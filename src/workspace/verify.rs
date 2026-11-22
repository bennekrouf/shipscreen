//! What a build actually produced, checked before anything is uploaded: the
//! signer, identifiers, version and target API of the AAB, and the signer,
//! profile and version of the IPA. A mistake here would otherwise surface as
//! a store rejection, or not at all.

use super::*;
use std::path::Path;
use std::process::Command;

fn check(label: &'static str, ok: bool, detail: impl Into<String>) -> CredCheck {
    CredCheck {
        label,
        ok,
        detail: detail.into(),
    }
}

/// SHA-256 fingerprint of the certificate that signed a jar-signed file (AAB).
pub(super) fn signer_fingerprint(file: &Path) -> Option<String> {
    let out = Command::new(signing::jdk_tool("keytool"))
        .quiet()
        .args(["-printcert", "-jarfile"])
        .arg(file)
        .output()
        .ok()?;
    String::from_utf8_lossy(&out.stdout).lines().find_map(|l| {
        l.trim()
            .strip_prefix("SHA256:")
            .map(|f| f.trim().to_string())
    })
}

/// A `key = value` / `key = "value"` from the Gradle Kotlin file dx generates.
pub(super) fn gradle_value(gradle: &str, key: &str) -> Option<String> {
    gradle.lines().find_map(|l| {
        let (k, v) = l.trim().split_once('=')?;
        (k.trim() == key).then(|| v.trim().trim_matches('"').to_string())
    })
}

/// What the AAB's own manifest says. Read from the bundle rather than from
/// the Gradle file dx generates: every dx run rewrites that file, so after an
/// APK build or `dx serve` it no longer describes the bundle on disk.
#[derive(Debug, Default, PartialEq)]
pub(crate) struct AabManifest {
    pub package: Option<String>,
    pub version_code: Option<u32>,
    pub version_name: Option<String>,
    pub target_sdk: Option<u32>,
}

/// Reads `base/manifest/AndroidManifest.xml` (aapt2's protobuf XML) out of
/// an AAB. None when the file can't be read or isn't a bundle.
pub(crate) fn aab_manifest(aab: &Path) -> Option<AabManifest> {
    let out = Command::new("unzip")
        .quiet()
        .arg("-p")
        .arg(aab)
        .arg("base/manifest/AndroidManifest.xml")
        .output()
        .ok()?;
    if !out.status.success() || out.stdout.is_empty() {
        return None;
    }
    parse_proto_manifest(&out.stdout)
}

/// One protobuf field: (number, varint value or length-delimited bytes).
enum Field<'a> {
    Varint(u64),
    Bytes(&'a [u8]),
}

fn varint(buf: &[u8], pos: &mut usize) -> Option<u64> {
    let mut v = 0u64;
    for shift in (0..64).step_by(7) {
        let b = *buf.get(*pos)?;
        *pos += 1;
        v |= u64::from(b & 0x7f) << shift;
        if b & 0x80 == 0 {
            return Some(v);
        }
    }
    None
}

/// The fields of one protobuf message, in order. Stops at the first
/// malformed byte.
fn fields(buf: &[u8]) -> Vec<(u64, Field<'_>)> {
    let mut out = Vec::new();
    let mut pos = 0;
    while pos < buf.len() {
        let Some(key) = varint(buf, &mut pos) else {
            break;
        };
        let field = match key & 7 {
            0 => match varint(buf, &mut pos) {
                Some(v) => Field::Varint(v),
                None => break,
            },
            1 => {
                pos += 8;
                continue;
            }
            2 => {
                let Some(len) = varint(buf, &mut pos) else {
                    break;
                };
                let Some(bytes) = buf.get(pos..pos + len as usize) else {
                    break;
                };
                pos += len as usize;
                Field::Bytes(bytes)
            }
            5 => {
                pos += 4;
                continue;
            }
            _ => break,
        };
        out.push((key >> 3, field));
    }
    out
}

fn sub(buf: &[u8], number: u64) -> impl Iterator<Item = &[u8]> {
    fields(buf).into_iter().filter_map(move |(n, f)| match f {
        Field::Bytes(b) if n == number => Some(b),
        _ => None,
    })
}

fn text(buf: &[u8], number: u64) -> Option<String> {
    sub(buf, number)
        .next()
        .map(|b| String::from_utf8_lossy(b).into_owned())
}

/// An XmlElement's attribute value by local name: its source text, or the
/// compiled integer when aapt2 dropped the text.
fn attribute(element: &[u8], name: &str) -> Option<String> {
    sub(element, 4).find_map(|attr| {
        if text(attr, 2).as_deref() != Some(name) {
            return None;
        }
        if let Some(v) = text(attr, 3).filter(|v| !v.is_empty()) {
            return Some(v);
        }
        // compiled_item (6) → prim (7) → int_decimal (6) / int_hex (7)
        let prim = sub(attr, 6).next().and_then(|item| sub(item, 7).next())?;
        fields(prim).into_iter().find_map(|(n, f)| match f {
            Field::Varint(v) if n == 6 || n == 7 => Some((v as u32).to_string()),
            _ => None,
        })
    })
}

/// XmlNode (element = 1) → XmlElement { name = 3, attribute = 4, child = 5 }.
fn parse_proto_manifest(buf: &[u8]) -> Option<AabManifest> {
    let root = sub(buf, 1).next()?;
    if text(root, 3).as_deref() != Some("manifest") {
        return None;
    }
    let uses_sdk = sub(root, 5)
        .filter_map(|node| sub(node, 1).next())
        .find(|el| text(el, 3).as_deref() == Some("uses-sdk"));
    Some(AabManifest {
        package: attribute(root, "package"),
        version_code: attribute(root, "versionCode").and_then(|v| v.parse().ok()),
        version_name: attribute(root, "versionName"),
        target_sdk: uses_sdk
            .and_then(|el| attribute(el, "targetSdkVersion"))
            .and_then(|v| v.parse().ok()),
    })
}

/// Checks for the newest AAB. `expected_fingerprint` is the upload key's,
/// when its password is available to read it.
pub(super) fn verify_aab(
    aab: &Path,
    project: &Path,
    slug: &str,
    package: &str,
    expected_fingerprint: Option<&str>,
) -> Vec<CredCheck> {
    let mut checks = Vec::new();
    match (signer_fingerprint(aab), expected_fingerprint) {
        (None, _) => checks.push(check(
            "Signed",
            false,
            "Not signed — Play rejects unsigned bundles",
        )),
        (Some(fp), Some(want)) if fp.eq_ignore_ascii_case(want) => checks.push(check(
            "Signed with your upload key",
            true,
            format!("SHA-256 {}…", &fp[..fp.len().min(23)]),
        )),
        (Some(fp), Some(_)) => checks.push(check(
            "Signed with your upload key",
            false,
            format!(
                "Signed by a different key (SHA-256 {}…) — Play will reject it",
                &fp[..fp.len().min(23)]
            ),
        )),
        (Some(fp), None) => checks.push(check(
            "Signed",
            true,
            format!(
                "SHA-256 {}… (save the keystore password to compare it with your upload key)",
                &fp[..fp.len().min(23)]
            ),
        )),
    }

    // The bundle's own manifest; the Gradle file dx generates is only a
    // fallback, since any later dx run rewrites it.
    let manifest = aab_manifest(aab).unwrap_or_default();
    let gradle = std::fs::read_to_string(project.join(format!(
        "target/dx/{slug}/release/android/app/app/build.gradle.kts"
    )))
    .unwrap_or_default();
    let app_id = manifest
        .package
        .clone()
        .or_else(|| gradle_value(&gradle, "applicationId"));
    let target = manifest
        .target_sdk
        .or_else(|| gradle_value(&gradle, "targetSdk").and_then(|v| v.parse().ok()));
    let code = manifest
        .version_code
        .map(|c| c.to_string())
        .or_else(|| gradle_value(&gradle, "versionCode"));
    let name = manifest
        .version_name
        .clone()
        .or_else(|| gradle_value(&gradle, "versionName"));
    let Some(app_id) = app_id else {
        checks.push(check(
            "Build settings",
            false,
            "Couldn't read the bundle's manifest — rebuild to check package, version and target API",
        ));
        return checks;
    };
    checks.push(check(
        "Package",
        !package.is_empty() && app_id == package,
        if app_id == package {
            app_id.clone()
        } else {
            format!("Built as {app_id}, but the app is {package}")
        },
    ));
    let target = target.unwrap_or(0);
    checks.push(check(
        "Target API",
        target >= 36,
        if target >= 36 {
            format!("API {target}")
        } else {
            format!("API {target} — Google Play requires 36 for updates")
        },
    ));
    let code = code.unwrap_or_default();
    let name = name.unwrap_or_default();
    checks.push(check(
        "Version",
        !code.is_empty(),
        format!("{name} ({code})"),
    ));
    checks
}

/// Checks for the newest IPA: identifiers, version, signer and profile.
pub(super) fn verify_ipa(ipa: &Path, bundle_id: &str) -> Vec<CredCheck> {
    let mut checks = Vec::new();
    let tmp = std::env::temp_dir().join(format!("shipscreen-ipa-check-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&tmp);
    let unzipped = Command::new("unzip")
        .args(["-q", "-o"])
        .arg(ipa)
        .arg("-d")
        .arg(&tmp)
        .status()
        .is_ok_and(|s| s.success());
    let app = std::fs::read_dir(tmp.join("Payload")).ok().and_then(|d| {
        d.flatten()
            .map(|e| e.path())
            .find(|p| p.extension().is_some_and(|e| e == "app"))
    });
    let Some(app) = app.filter(|_| unzipped) else {
        let _ = std::fs::remove_dir_all(&tmp);
        return vec![check("Readable", false, "Could not open the IPA")];
    };

    let plist = Command::new("plutil")
        .args(["-convert", "json", "-o", "-"])
        .arg(app.join("Info.plist"))
        .output()
        .ok()
        .and_then(|o| serde_json::from_slice::<serde_json::Value>(&o.stdout).ok())
        .unwrap_or_default();
    let built_id = plist["CFBundleIdentifier"].as_str().unwrap_or("");
    checks.push(check(
        "Bundle ID",
        built_id == bundle_id,
        if built_id == bundle_id {
            built_id.to_string()
        } else {
            format!("Built as {built_id}, but the app is {bundle_id}")
        },
    ));
    checks.push(check(
        "Version",
        plist["CFBundleShortVersionString"].is_string() && plist["CFBundleVersion"].is_string(),
        format!(
            "{} ({})",
            plist["CFBundleShortVersionString"].as_str().unwrap_or("?"),
            plist["CFBundleVersion"].as_str().unwrap_or("?")
        ),
    ));

    let sig = Command::new("codesign")
        .args(["-dvv"])
        .arg(&app)
        .output()
        .ok();
    let authority = sig
        .as_ref()
        .map(|o| String::from_utf8_lossy(&o.stderr).to_string())
        .and_then(|t| {
            t.lines()
                .find_map(|l| l.strip_prefix("Authority=").map(str::to_string))
        })
        .unwrap_or_default();
    checks.push(check(
        "Signed for distribution",
        authority.starts_with("Apple Distribution") || authority.starts_with("iPhone Distribution"),
        if authority.is_empty() {
            "Not signed".to_string()
        } else {
            authority
        },
    ));

    match read_profile(&app.join("embedded.mobileprovision")) {
        Some(p) => {
            let ok = p.kind == ProfileKind::AppStore && p.matches_bundle(bundle_id) && !p.expired();
            checks.push(check(
                "Provisioning profile",
                ok,
                format!("{} · {} · expires {}", p.name, p.kind.label(), p.expires),
            ));
        }
        None => checks.push(check("Provisioning profile", false, "No embedded profile")),
    }
    let _ = std::fs::remove_dir_all(&tmp);
    checks
}

#[cfg(test)]
mod tests {
    use super::*;

    fn len_field(n: u64, body: &[u8]) -> Vec<u8> {
        let mut out = vec![((n << 3) | 2) as u8];
        let mut len = body.len();
        loop {
            let b = (len & 0x7f) as u8;
            len >>= 7;
            if len == 0 {
                out.push(b);
                break;
            }
            out.push(b | 0x80);
        }
        out.extend_from_slice(body);
        out
    }

    fn attr(name: &str, value: &str) -> Vec<u8> {
        let mut a = len_field(1, b"http://schemas.android.com/apk/res/android");
        a.extend(len_field(2, name.as_bytes()));
        a.extend(len_field(3, value.as_bytes()));
        len_field(4, &a)
    }

    #[test]
    fn reads_the_proto_manifest_of_a_bundle() {
        let mut sdk = len_field(3, b"uses-sdk");
        sdk.extend(attr("minSdkVersion", "24"));
        sdk.extend(attr("targetSdkVersion", "36"));
        let mut root = len_field(3, b"manifest");
        root.extend(attr("package", "com.mayorana.scan2fil"));
        root.extend(attr("versionCode", "3"));
        root.extend(attr("versionName", "1.0"));
        root.extend(len_field(5, &len_field(1, &sdk)));
        // versionCode compiled only: compiled_item → prim → int_decimal = 12
        let mut compiled = len_field(2, b"versionCode");
        compiled.extend(len_field(6, &len_field(7, &[6 << 3, 12])));
        let only_compiled = [len_field(3, b"manifest"), len_field(4, &compiled)].concat();

        assert_eq!(
            parse_proto_manifest(&len_field(1, &root)),
            Some(AabManifest {
                package: Some("com.mayorana.scan2fil".into()),
                version_code: Some(3),
                version_name: Some("1.0".into()),
                target_sdk: Some(36),
            })
        );
        assert_eq!(
            parse_proto_manifest(&len_field(1, &only_compiled)).and_then(|m| m.version_code),
            Some(12)
        );
        assert_eq!(parse_proto_manifest(b"not a manifest"), None);
    }

    #[test]
    fn reads_gradle_values() {
        let g = "android {\n    namespace = \"com.a\"\n    defaultConfig {\n        applicationId = \"com.mayorana.nahw\"\n        targetSdk = 36\n        versionCode = 7\n        versionName = \"1.0.0\"\n    }\n}";
        assert_eq!(
            gradle_value(g, "applicationId").as_deref(),
            Some("com.mayorana.nahw")
        );
        assert_eq!(gradle_value(g, "targetSdk").as_deref(), Some("36"));
        assert_eq!(gradle_value(g, "versionName").as_deref(), Some("1.0.0"));
        assert_eq!(gradle_value(g, "minSdk"), None);
    }

    #[test]
    fn detects_the_signer_of_a_signed_jar() {
        if signing::java_home().is_none() {
            return; // no JDK
        }
        let dir = std::env::temp_dir().join("shipscreen-test-verify");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let ks = dir.join("k.jks");
        signing::create_upload_key(&ks, "a", "test-pass-123", "CN=Test").unwrap();
        let jar = dir.join("x.aab");
        std::fs::write(dir.join("f.txt"), "x").unwrap();
        assert!(Command::new(signing::jdk_tool("jar"))
            .arg("cf")
            .arg(&jar)
            .arg("-C")
            .arg(&dir)
            .arg("f.txt")
            .status()
            .unwrap()
            .success());
        assert_eq!(signer_fingerprint(&jar), None, "unsigned before jarsigner");
        let signed = Command::new(signing::jdk_tool("jarsigner"))
            .arg("-keystore")
            .arg(&ks)
            .args(["-storepass:env", "P"])
            .env("P", "test-pass-123")
            .arg(&jar)
            .arg("a")
            .output()
            .unwrap();
        assert!(
            signed.status.success(),
            "{}",
            String::from_utf8_lossy(&signed.stdout)
        );
        let want = signing::upload_key_fingerprint(&ks, "a", "test-pass-123").unwrap();
        assert_eq!(signer_fingerprint(&jar).as_deref(), Some(want.as_str()));
        let checks = verify_aab(&jar, &dir, "none", "com.x", Some(&want));
        assert!(checks[0].ok, "{:?}", checks[0]);
        assert!(!verify_aab(&jar, &dir, "none", "com.x", Some("AA:BB"))[0].ok);
    }
}
