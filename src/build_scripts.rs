//! The build scripts ShipScreen writes into a project: iOS distribution, Android release and APK.

/// Generate the script content for `build_ios_distribution.sh`.
/// Only the 4 top variables are substituted; everything else is verbatim.
pub(super) fn script_ios_distribution(
    app_name: &str,
    project_slug: &str,
    bundle_id: &str,
    identity: &str,
    profile_path: &str,
    short_version: &str,
    build_number: u32,
) -> String {
    format!(
        r##"#!/bin/bash

# build_ios_distribution.sh - Consolidated Build Script for {app_name}
# Handles building, validation fixes, signing (with Entitlements), and packaging.

set -e

APP_NAME="{app_name}"
# Bundle Identifier
BUNDLE_ID="{bundle_id}"

# Signing Identity (Distribution)
IDENTITY="{identity}"

# Paths
OUTPUT_DIR="target/ios/ipa"
DX_IOS_DIR="target/dx/{project_slug}/release/ios"
ENTITLEMENTS="Entitlements.plist"

echo "🚀 Starting iOS Build for App Store Distribution..."

# 1. Prerequisite Checks
if ! command -v dx &>/dev/null; then
  echo "❌ Dioxus CLI (dx) not found. Please install it."
  exit 1
fi

# 1.5 Normalize icon to genuine PNG format
# Apple's validator reads the actual file format, not just the extension.
# A JPEG renamed to .png will pass the filename check but fail validation.
if [ -f "assets/icon.png" ]; then
  sips -s format png "assets/icon.png" --out "assets/icon.png" >/dev/null 2>&1
  echo "✅ Icon normalized to true PNG format."
fi

# 2. Build Rust Project for iOS (Release)
echo "🧹 Cleaning old builds..."
rm -rf target/dx/{project_slug}/release/ios
rm -rf target/dx/{project_slug}/release/web # Clean web assets cache too

echo "📦 Building Rust project for iOS (Release - Device)..."
# Force aarch64-apple-ios to avoid simulator slices
dx build --platform ios --release --target aarch64-apple-ios

# 3. Locate Generated App Bundle (must be after dx build)
APP_BUNDLE=$(find "$DX_IOS_DIR" -maxdepth 1 -name "*.app" -type d 2>/dev/null | head -1)
if [ -z "$APP_BUNDLE" ] || [ ! -d "$APP_BUNDLE" ]; then
  echo "❌ Could not find generated .app bundle in $DX_IOS_DIR"
  ls "$DX_IOS_DIR" 2>/dev/null || echo "   (directory does not exist)"
  exit 1
fi
echo "✅ Found App Bundle: $APP_BUNDLE"

# 4. Prepare Payload Directory
echo "📦 Packaging IPA structure..."
mkdir -p "$OUTPUT_DIR"
rm -rf "$OUTPUT_DIR/Payload"
mkdir -p "$OUTPUT_DIR/Payload"

# Copy App Bundle to Payload
cp -R "$APP_BUNDLE" "$OUTPUT_DIR/Payload/"

# Define paths for the copied app
APP_PATH="$OUTPUT_DIR/Payload/$(basename "$APP_BUNDLE")"
PLIST_PATH="$APP_PATH/Info.plist"

# 4.5 Generate App Icons via actool → Assets.car + correct plist
# The root cause of "Missing icon" validation failures was that actool requires
# the output directory to exist before running, and was silently failing.
echo "🎨 Generating App Icons..."
ICON_SOURCE="assets/icon.png"

if [ -f "$ICON_SOURCE" ]; then
  echo "   Found source icon: $ICON_SOURCE"

  # Convert to true PNG and strip alpha (Apple rejects transparency and JPEG-as-PNG)
  FLAT_ICON="/tmp/icon_flat_$$.png"
  sips -s format png "$ICON_SOURCE" --out "$FLAT_ICON" >/dev/null 2>&1
  echo "   Converted to true PNG."

  # Build asset catalog in temp dir
  CATALOG_DIR="/tmp/shipscreen_icons_$$"
  ICON_SET="$CATALOG_DIR/Assets.xcassets/AppIcon.appiconset"
  mkdir -p "$ICON_SET"

  # iPhone sizes
  sips -z 40   40   "$FLAT_ICON" --out "$ICON_SET/Icon-20@2x.png"      >/dev/null
  sips -z 60   60   "$FLAT_ICON" --out "$ICON_SET/Icon-20@3x.png"      >/dev/null
  sips -z 58   58   "$FLAT_ICON" --out "$ICON_SET/Icon-29@2x.png"      >/dev/null
  sips -z 87   87   "$FLAT_ICON" --out "$ICON_SET/Icon-29@3x.png"      >/dev/null
  sips -z 80   80   "$FLAT_ICON" --out "$ICON_SET/Icon-40@2x.png"      >/dev/null
  sips -z 120  120  "$FLAT_ICON" --out "$ICON_SET/Icon-40@3x.png"      >/dev/null
  sips -z 120  120  "$FLAT_ICON" --out "$ICON_SET/Icon-60@2x.png"      >/dev/null
  sips -z 180  180  "$FLAT_ICON" --out "$ICON_SET/Icon-60@3x.png"      >/dev/null
  # iPad sizes
  sips -z 20   20   "$FLAT_ICON" --out "$ICON_SET/Icon-ipad-20@1x.png" >/dev/null
  sips -z 40   40   "$FLAT_ICON" --out "$ICON_SET/Icon-ipad-20@2x.png" >/dev/null
  sips -z 29   29   "$FLAT_ICON" --out "$ICON_SET/Icon-ipad-29@1x.png" >/dev/null
  sips -z 58   58   "$FLAT_ICON" --out "$ICON_SET/Icon-ipad-29@2x.png" >/dev/null
  sips -z 40   40   "$FLAT_ICON" --out "$ICON_SET/Icon-ipad-40@1x.png" >/dev/null
  sips -z 80   80   "$FLAT_ICON" --out "$ICON_SET/Icon-ipad-40@2x.png" >/dev/null
  sips -z 76   76   "$FLAT_ICON" --out "$ICON_SET/Icon-ipad-76@1x.png" >/dev/null
  sips -z 152  152  "$FLAT_ICON" --out "$ICON_SET/Icon-ipad-76@2x.png" >/dev/null
  sips -z 167  167  "$FLAT_ICON" --out "$ICON_SET/Icon-ipad-83@2x.png" >/dev/null
  # App Store
  sips -z 1024 1024 "$FLAT_ICON" --out "$ICON_SET/Icon-1024.png"       >/dev/null
  rm -f "$FLAT_ICON"

  cat > "$ICON_SET/Contents.json" << 'ICONEOF'
{{
  "images": [
    {{"idiom":"iphone","scale":"2x","size":"20x20","filename":"Icon-20@2x.png"}},
    {{"idiom":"iphone","scale":"3x","size":"20x20","filename":"Icon-20@3x.png"}},
    {{"idiom":"iphone","scale":"2x","size":"29x29","filename":"Icon-29@2x.png"}},
    {{"idiom":"iphone","scale":"3x","size":"29x29","filename":"Icon-29@3x.png"}},
    {{"idiom":"iphone","scale":"2x","size":"40x40","filename":"Icon-40@2x.png"}},
    {{"idiom":"iphone","scale":"3x","size":"40x40","filename":"Icon-40@3x.png"}},
    {{"idiom":"iphone","scale":"2x","size":"60x60","filename":"Icon-60@2x.png"}},
    {{"idiom":"iphone","scale":"3x","size":"60x60","filename":"Icon-60@3x.png"}},
    {{"idiom":"ipad","scale":"1x","size":"20x20","filename":"Icon-ipad-20@1x.png"}},
    {{"idiom":"ipad","scale":"2x","size":"20x20","filename":"Icon-ipad-20@2x.png"}},
    {{"idiom":"ipad","scale":"1x","size":"29x29","filename":"Icon-ipad-29@1x.png"}},
    {{"idiom":"ipad","scale":"2x","size":"29x29","filename":"Icon-ipad-29@2x.png"}},
    {{"idiom":"ipad","scale":"1x","size":"40x40","filename":"Icon-ipad-40@1x.png"}},
    {{"idiom":"ipad","scale":"2x","size":"40x40","filename":"Icon-ipad-40@2x.png"}},
    {{"idiom":"ipad","scale":"1x","size":"76x76","filename":"Icon-ipad-76@1x.png"}},
    {{"idiom":"ipad","scale":"2x","size":"76x76","filename":"Icon-ipad-76@2x.png"}},
    {{"idiom":"ipad","scale":"2x","size":"83.5x83.5","filename":"Icon-ipad-83@2x.png"}},
    {{"idiom":"ios-marketing","scale":"1x","size":"1024x1024","filename":"Icon-1024.png"}}
  ],
  "info": {{"author":"xcode","version":1}}
}}
ICONEOF

  # APP_PATH already exists (bundle was copied there). actool writes Assets.car into it.
  PARTIAL_PLIST="/tmp/partial_info_$$.plist"
  echo "   Compiling asset catalog with actool..."
  xcrun actool "$CATALOG_DIR/Assets.xcassets" \
    --compile "$APP_PATH" \
    --platform iphoneos \
    --minimum-deployment-target 14.0 \
    --app-icon AppIcon \
    --output-partial-info-plist "$PARTIAL_PLIST" 2>&1 | grep -v "^$" || true

  if [ -f "$PARTIAL_PLIST" ]; then
    # Remove existing icon keys then merge actool-generated ones
    /usr/libexec/PlistBuddy -c "Delete :CFBundleIcons" "$PLIST_PATH" 2>/dev/null || true
    /usr/libexec/PlistBuddy -c "Delete :CFBundleIcons~ipad" "$PLIST_PATH" 2>/dev/null || true
    /usr/libexec/PlistBuddy -c "Delete :CFBundleIconName" "$PLIST_PATH" 2>/dev/null || true
    /usr/libexec/PlistBuddy -c "Merge $PARTIAL_PLIST" "$PLIST_PATH"
    rm -f "$PARTIAL_PLIST"
    echo "✅ Assets.car generated and Info.plist updated."
  else
    echo "⚠️  actool did not produce a partial plist."
  fi

  rm -rf "$CATALOG_DIR"
else
  echo "⚠️  Warning: assets/icon.png not found. Skipping icon generation."
fi

echo "🔧 Fixing Info.plist for App Store Validation..."

# Remove empty lines at start of file
sed -i '' '/^$/d' "$PLIST_PATH"

# Inject Bundle Metadata
plutil -replace CFBundleIdentifier -string "$BUNDLE_ID" "$PLIST_PATH"
plutil -replace CFBundleDisplayName -string "{app_name}" "$PLIST_PATH"

# Build Number — owned by ShipScreen (Version step), injected per build
BUILD_NUMBER="{build_number}"
echo "   Version {short_version} ($BUILD_NUMBER)"

plutil -replace CFBundleVersion -string "$BUILD_NUMBER" "$PLIST_PATH"
plutil -replace CFBundleShortVersionString -string "{short_version}" "$PLIST_PATH"
plutil -replace MinimumOSVersion -string "14.0" "$PLIST_PATH"
plutil -replace CFBundlePackageType -string "APPL" "$PLIST_PATH"

# Enforce Single Supported Platform (Fixes Validation Error 90562)
plutil -replace CFBundleSupportedPlatforms -xml "<array><string>iPhoneOS</string></array>" "$PLIST_PATH"

# Encryption Export Compliance (Fixes Missing Compliance Warning)
plutil -replace ITSAppUsesNonExemptEncryption -bool NO "$PLIST_PATH"

# Inject UILaunchScreen (Requires iOS 14.0+)
plutil -remove UILaunchStoryboardName "$PLIST_PATH" || true
plutil -replace UILaunchScreen -xml "<dict>
        <key>UIColorName</key>
        <string>LaunchBackgroundColor</string>
        <key>UIImageName</key>
        <string>LaunchImage</string>
    </dict>" "$PLIST_PATH"

# Dynamic SDK/Platform Metadata
SDK_VERSION=$(xcrun --sdk iphoneos --show-sdk-version)
SDK_BUILD=$(xcrun --sdk iphoneos --show-sdk-build-version)

XCODE_VERSION_RAW=$(xcodebuild -version | grep "Xcode" | awk '{{print $2}}')
XCODE_BUILD=$(xcodebuild -version | grep "Build version" | awk '{{print $3}}')

MAJOR=$(echo "$XCODE_VERSION_RAW" | cut -d. -f1)
MINOR=$(echo "$XCODE_VERSION_RAW" | cut -d. -f2)
DT_XCODE="${{MAJOR}}${{MINOR}}0"

echo "   Detected Xcode: $XCODE_VERSION_RAW ($XCODE_BUILD) -> DTXcode: $DT_XCODE"
echo "   Detected SDK: iOS $SDK_VERSION ($SDK_BUILD)"

plutil -replace DTPlatformName -string "iphoneos" "$PLIST_PATH"
plutil -replace DTPlatformVersion -string "$SDK_VERSION" "$PLIST_PATH"
plutil -replace DTSDKName -string "iphoneos$SDK_VERSION" "$PLIST_PATH"
plutil -replace DTSDKBuild -string "$SDK_BUILD" "$PLIST_PATH"
plutil -replace DTPlatformBuild -string "$SDK_BUILD" "$PLIST_PATH"

plutil -replace DTXcode -string "$DT_XCODE" "$PLIST_PATH"
plutil -replace DTXcodeBuild -string "$XCODE_BUILD" "$PLIST_PATH"
plutil -replace DTCompiler -string "com.apple.compilers.llvm.clang.1_0" "$PLIST_PATH"

# 6. Embed Provisioning Profile
# Priority order:
#   1. ~/Downloads/<AppName>.mobileprovision  (project-specific, freshly downloaded)
#   2. ShipScreen global profile selection
#   3. embedded.mobileprovision in project root
#   4. Any .mobileprovision in project root
GLOBAL_PROFILE="{profile_path}"
NAMED_PROFILE=$(ls "$HOME/Downloads/{app_name}.mobileprovision" 2>/dev/null | head -1)

if [ -f "$NAMED_PROFILE" ]; then
  echo "📄 Using project profile from Downloads: $NAMED_PROFILE"
  cp "$NAMED_PROFILE" "$APP_PATH/embedded.mobileprovision"
elif [ -f "$GLOBAL_PROFILE" ]; then
  echo "📄 Using global profile: $GLOBAL_PROFILE"
  cp "$GLOBAL_PROFILE" "$APP_PATH/embedded.mobileprovision"
elif [ -f "embedded.mobileprovision" ]; then
  echo "📄 Found embedded.mobileprovision in root"
  cp "embedded.mobileprovision" "$APP_PATH/embedded.mobileprovision"
else
  PROVISION_PROFILE=$(find . -maxdepth 1 -name "*.mobileprovision" | head -n 1)
  if [ -f "$PROVISION_PROFILE" ]; then
    echo "📄 Found profile: $PROVISION_PROFILE"
    cp "$PROVISION_PROFILE" "$APP_PATH/embedded.mobileprovision"
  else
    echo "❌  ERROR: No .mobileprovision file found. Signing will fail!"
    exit 1
  fi
fi

# 6.5 Remove stale/duplicate assets
echo "🧹 Removing stale assets from app bundle..."
CSS_COUNT=$(find "$APP_PATH/assets" -name "*.css" 2>/dev/null | wc -l | tr -d ' ')
if [ "$CSS_COUNT" -gt 1 ]; then
  echo "   ⚠️  Found $CSS_COUNT CSS files - keeping only the newest"
  find "$APP_PATH/assets" -name "*.css" -print0 | xargs -0 ls -t | tail -n +2 | xargs rm -f
  echo "   ✅ Cleaned stale CSS files"
fi

# 7. Codesign with Entitlements
echo "✍️  Signing with identity: $IDENTITY"
echo "   Entitlements: $ENTITLEMENTS"

if [ ! -f "$ENTITLEMENTS" ]; then
  echo "❌ Entitlements file not found at $ENTITLEMENTS"
  exit 1
fi

# Remove existing signature
rm -rf "$APP_PATH/_CodeSignature"

# Codesign
codesign --force --deep --sign "$IDENTITY" --entitlements "$ENTITLEMENTS" --timestamp "$APP_PATH"

# Verify signature
echo "🔍 Verifying signature..."
codesign --verify --deep --strict --verbose=4 "$APP_PATH" 2>&1 || {{
  echo "❌ Signature verification failed"
  exit 1
}}

# 8. Create IPA
echo "📦 Creating .ipa..."
rm -f "$OUTPUT_DIR/$APP_NAME.ipa"
rm -f "./$APP_NAME.ipa"
pushd "$OUTPUT_DIR" >/dev/null
zip -r "$APP_NAME.ipa" Payload >/dev/null
popd >/dev/null

rm -rf "$OUTPUT_DIR/Payload"

echo "✅ Build Complete!"
echo "📂 IPA location: $OUTPUT_DIR/$APP_NAME.ipa"

cp "$OUTPUT_DIR/$APP_NAME.ipa" "./$APP_NAME.ipa"
echo "✅ IPA copied to project root: ./$APP_NAME.ipa"
open -R "./$APP_NAME.ipa"
"##
    )
}

pub(super) fn script_android_release(
    app_name: &str,
    project_slug: &str,
    bundle_id: &str,
    version_name: &str,
    version_code: u32,
) -> String {
    // Signing comes from the project's environment (.env), never from this
    // template: the generated script sits in the project folder and this
    // source is public. The key alias defaults to the slug.
    let slug_lower = project_slug.to_lowercase();

    // android_bundle_id is passed directly — callers already resolved iOS vs Android split.
    let android_package = bundle_id;

    // OLD_PACKAGE is what dx generates by default (com.example.Title-cased slug)
    let title_case = {
        let mut c = project_slug.chars();
        match c.next() {
            None => String::new(),
            Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
        }
    };
    let old_package = format!("com.example.{title_case}");
    format!(
        r##"#!/bin/bash

# build_android_release.sh - Build Signed Android App Bundle (AAB) for Google Play
set -e

PROJECT_NAME="{project_slug}"

# Signing — ShipScreen passes these in from Accounts → Android upload key
# (password from the Keychain); run by hand, set them in the project's .env:
#   ANDROID_KEYSTORE_PATH       upload keystore (.jks / .keystore)
#   ANDROID_KEYSTORE_PASSWORD   its password
#   ANDROID_KEY_ALIAS           optional, defaults to the project slug
#   ANDROID_KEY_PASSWORD        optional, defaults to the keystore password
KEYSTORE_PATH="${{ANDROID_KEYSTORE_PATH:-}}"
KEYSTORE_PATH="${{KEYSTORE_PATH/#\~/$HOME}}"
KEY_ALIAS="${{ANDROID_KEY_ALIAS:-{slug_lower}}}"
KEY_PASS="${{ANDROID_KEYSTORE_PASSWORD:-}}"
KEY_PASSWORD="${{ANDROID_KEY_PASSWORD:-$KEY_PASS}}"

export ANDROID_HOME="$HOME/Library/Android/sdk"
export ANDROID_NDK_HOME="$ANDROID_HOME/ndk"
NDK_VERSION=$(ls -1 "$ANDROID_NDK_HOME" 2>/dev/null | grep -E '^[0-9]+\.' | sort -V | tail -1)
export ANDROID_NDK_HOME="$ANDROID_NDK_HOME/$NDK_VERSION"

echo "🚀 Starting Android Release Build (AAB)..."
echo "NDK: $NDK_VERSION"

# 0. Check tools
if [ -z "$NDK_VERSION" ]; then
    echo "❌ Android NDK not found in $ANDROID_HOME/ndk"
    echo "   Install it in Android Studio: SDK Manager → SDK Tools → NDK (Side by side)."
    exit 1
fi
# dx refuses to bundle a project whose Dioxus library is a different version.
DX_VERSION=$(dx --version 2>/dev/null | awk '{{print $2}}')
LIB_VERSION=$(awk '/^name = "dioxus"$/ {{ getline; gsub(/version = |"/, ""); print; exit }}' Cargo.lock 2>/dev/null)
if [ -n "$DX_VERSION" ] && [ -n "$LIB_VERSION" ] && [ "$DX_VERSION" != "$LIB_VERSION" ]; then
    echo "❌ dx is $DX_VERSION but this project uses dioxus $LIB_VERSION — they must match."
    echo "   Either move the project to dx's version:  cargo update -p dioxus --precise $DX_VERSION"
    echo "   or install the matching dx:               cargo install dioxus-cli --version $LIB_VERSION --locked"
    exit 1
fi

# 0b. Check signing
if [ -z "$KEYSTORE_PATH" ] || [ -z "$KEY_PASS" ]; then
    echo "❌ Android signing is not set up."
    echo "   In ShipScreen: Accounts → Android upload key (the password stays in the Keychain)."
    echo "   Running this script by hand? Add to the project's .env:"
    echo "   ANDROID_KEYSTORE_PATH=/path/to/upload-keystore.jks"
    echo "   ANDROID_KEYSTORE_PASSWORD=<its password>"
    exit 1
fi
if [ ! -f "$KEYSTORE_PATH" ]; then
    echo "❌ Keystore not found: $KEYSTORE_PATH"
    echo "   Create an upload key with:"
    echo "   keytool -genkeypair -v -keystore \"$KEYSTORE_PATH\" -alias $KEY_ALIAS -keyalg RSA -keysize 2048 -validity 10000"
    exit 1
fi

# 1. Start Build
echo "🧹 Cleaning previous release build..."
rm -rf "target/dx/$PROJECT_NAME/release/android"

echo "📦 Running dx bundle..."
# dx's own final Gradle step can fail (duplicate icons); that is patched and
# rebuilt below. What must exist is the Gradle project dx generates first.
dx bundle --platform android --release || true
if [ ! -f "target/dx/$PROJECT_NAME/release/android/app/app/build.gradle.kts" ]; then
    echo "❌ dx bundle stopped before generating the Android project — see its error above."
    exit 1
fi

# 2. Resource & Icon Fixes
echo "🎨 Fixing resources and icons..."
RES_DIR="target/dx/$PROJECT_NAME/release/android/app/app/src/main/res"
SOURCE_ICONS="manual_assets/android_icons"

if [ -d "$RES_DIR" ] && [ -d "$SOURCE_ICONS" ]; then
    find "$RES_DIR" -name "ic_launcher.webp" -delete
    find "$RES_DIR" -name "ic_launcher_round.webp" -delete
    # Remove adaptive icon XMLs — they override PNGs on Android 8+ and show the default robot icon
    rm -f "$RES_DIR/mipmap-anydpi-v26/ic_launcher.xml"
    rm -f "$RES_DIR/mipmap-anydpi-v26/ic_launcher_round.xml"
    cp -r "$SOURCE_ICONS/mipmap-"* "$RES_DIR/"
    echo "✅ Icons updated."
else
    echo "⚠️  Warning: Resources directory not found."
fi

# 3. Package Name Fixes
echo "🔧 Fixing Package Name..."
BUILD_DIR="target/dx/$PROJECT_NAME/release/android/app"
BUILD_GRADLE="$BUILD_DIR/app/build.gradle.kts"
OLD_PACKAGE="{old_package}"
NEW_PACKAGE="{android_package}"

# Version info — owned by ShipScreen (Version step), injected per build.
# Mirrored into Dioxus.toml so the project's own config stays in step.
V_CODE={version_code}
V_NAME="{version_name}"
sed -i '' "s/version_code *= *[0-9]*/version_code = $V_CODE/g" Dioxus.toml
sed -i '' "s/version_name *= *\".*\"/version_name = \"$V_NAME\"/g" Dioxus.toml

echo "   Package: $NEW_PACKAGE"
echo "   Version: $V_NAME ($V_CODE)"

if grep -q "$OLD_PACKAGE" "$BUILD_GRADLE" || grep -q "versionCode" "$BUILD_GRADLE"; then
    echo "   Updating build.gradle.kts..."
    # Fix package name
    sed -i '' "s/$OLD_PACKAGE/$NEW_PACKAGE/g" "$BUILD_GRADLE"
    
    # Fix Version Code
    sed -i '' "s/versionCode *= *[0-9]*/versionCode = $V_CODE/g" "$BUILD_GRADLE"
    
    # Fix Version Name
    sed -i '' "s/versionName *= *\".*\"/versionName = \"$V_NAME\"/g" "$BUILD_GRADLE"

    find "$BUILD_DIR/app/src" -name "*.kt" -type f -exec sed -i '' "s/$OLD_PACKAGE/$NEW_PACKAGE/g" {{}} \;
    find "$BUILD_DIR/app/src" -name "AndroidManifest.xml" -type f -exec sed -i '' "s/$OLD_PACKAGE/$NEW_PACKAGE/g" {{}} \;
    find "$BUILD_DIR/app/src" -name "AndroidManifest.xml" -type f -exec sed -i '' "s/android:label=\\\"@string\\/app_name\\\"/android:label=\\\"{app_name}\\\"/g" {{}} \;
fi

# 4. Inject Signing Config
echo "✍️  Injecting Signing Configuration..."

if ! grep -q "signingConfigs" "$BUILD_GRADLE"; then
    ABS_KEYSTORE_PATH="$KEYSTORE_PATH"

    if [[ "$(cat "$BUILD_GRADLE")" == *"buildTypes"* ]]; then
        echo "   Patching existing buildTypes..."
        sed -i '' "/buildTypes {{/i \\
        signingConfigs {{\\
            create(\"release\") {{\\
                storeFile = file(\"$ABS_KEYSTORE_PATH\")\\
                storePassword = \"$KEY_PASS\"\\
                keyAlias = \"$KEY_ALIAS\"\\
                keyPassword = \"$KEY_PASSWORD\"\\
            }}\\
        }}\\
" "$BUILD_GRADLE"

        sed -i '' "/getByName(\"release\") {{/a \\
            signingConfig = signingConfigs.getByName(\"release\")
" "$BUILD_GRADLE"
    else
        echo "⚠️  Could not find buildTypes block."
    fi
else
    echo "ℹ️  Signing config appears already present."
fi

# 5. Build AAB with Gradle
echo "🏗️  Building Android App Bundle..."
cd "$BUILD_DIR"
./gradlew bundleRelease

# 6. Verify and Move
# Standard Gradle output for signed release is 'app-release.aab'
# Do NOT use 'find' because it picks up the unsigned architecture-specific bundles instead of the final signed universal bundle.
OUTPUT_AAB="app/build/outputs/bundle/release/app-release.aab"
if [ -f "$OUTPUT_AAB" ]; then
    echo "✅ AAB Generated: $OUTPUT_AAB"
    cd - > /dev/null
    TARGET_NAME="${{PROJECT_NAME}}_release.aab"
    cp "$BUILD_DIR/$OUTPUT_AAB" "./$TARGET_NAME"
    echo "📦 Final Bundle copied to: ./$TARGET_NAME"

    echo "🔍 Verifying Signature..."
    if jarsigner -verify -verbose -certs "$TARGET_NAME" | grep -q "jar verified"; then
        echo "✅ Signature Verified!"
        echo "🎉 Ready for Google Play Console upload."
        open -R "./$TARGET_NAME"
        open "$(pwd)"
    else
        echo "❌ Signature Verification FAILED."
        exit 1
    fi
else
    echo "❌ Build Failed: AAB not found in app/build/outputs/bundle/release/"
    ls "$BUILD_DIR/app/build/outputs/bundle/release/" 2>/dev/null || echo "(directory does not exist)"
    exit 1
fi
"##
    )
}

/// Generate `build_android.sh` (release APK via Gradle assembleRelease)
pub(super) fn script_android(_app_name: &str, project_slug: &str, _bundle_id: &str) -> String {
    format!(
        r##"#!/bin/bash

# Exit on error
set -e

export ANDROID_HOME="$HOME/Library/Android/sdk"
export ANDROID_NDK_HOME="$ANDROID_HOME/ndk"
NDK_VERSION=$(ls -1 "$ANDROID_NDK_HOME" 2>/dev/null | grep -E '^[0-9]+\.' | sort -V | tail -1)
export ANDROID_NDK_HOME="$ANDROID_NDK_HOME/$NDK_VERSION"

echo "=== Building Android APK ==="
echo "NDK: $NDK_VERSION"
echo

if ! dx bundle --platform android --release; then
    echo "⚠️ dx bundle failed. This is expected due to duplicate icons."
    echo "Attempting to resolve duplicate resources..."
fi

TARGET_RES="target/dx/{project_slug}/release/android/app/app/src/main/res"
SOURCE_ICONS="assets/icons/android"

if [ -d "$TARGET_RES" ] && [ -d "$SOURCE_ICONS" ]; then
    echo "🧹 Cleaning up ALL existing launcher icons in target..."
    find "$TARGET_RES" -name "ic_launcher.webp" -delete
    find "$TARGET_RES" -name "ic_launcher.png" -delete
    find "$TARGET_RES" -name "ic_launcher_round.webp" -delete
    find "$TARGET_RES" -name "ic_launcher_round.png" -delete
    echo "📂 Copying correct icons from $SOURCE_ICONS..."
    cp -R "$SOURCE_ICONS/"* "$TARGET_RES/"
else
    echo "❌ Target resources or source icons not found. Build may fail."
fi

echo "Resuming build with Gradle..."
cd target/dx/{project_slug}/release/android/app
./gradlew assembleRelease
cd -

echo
echo "✅ Build complete"
"##
    )
}

/// Generate `build_apk.sh` (debug/installable APK)
pub(super) fn script_build_apk(app_name: &str, project_slug: &str, bundle_id: &str) -> String {
    // android_bundle_id is passed directly — callers already resolved iOS vs Android split.
    let android_package = bundle_id;
    let title_case = {
        let mut c = project_slug.chars();
        match c.next() {
            None => String::new(),
            Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
        }
    };
    let old_package = format!("com.example.{title_case}");
    format!(
        r##"#!/bin/bash

# build_apk.sh - Build Android APK (Debug/Installable) for {app_name}
# Use this for local testing on a device/emulator

set -e

export ANDROID_HOME="$HOME/Library/Android/sdk"
export ANDROID_NDK_HOME="$ANDROID_HOME/ndk"
NDK_VERSION=$(ls -1 "$ANDROID_NDK_HOME" 2>/dev/null | grep -E '^[0-9]+\.' | sort -V | tail -1)
export ANDROID_NDK_HOME="$ANDROID_NDK_HOME/$NDK_VERSION"
BUILD_TOOLS_HOME="$ANDROID_HOME/build-tools"
LATEST_BUILD_TOOLS=$(ls -1 "$BUILD_TOOLS_HOME" 2>/dev/null | sort -V | tail -1)
ANDROID_BUILD_TOOLS="$BUILD_TOOLS_HOME/$LATEST_BUILD_TOOLS"
ZIPALIGN="$ANDROID_BUILD_TOOLS/zipalign"
APKSIGNER="$ANDROID_BUILD_TOOLS/apksigner"

PROJECT_NAME="{project_slug}"

echo "=== Building Android APK (Debug) for $PROJECT_NAME ==="
echo "NDK: $NDK_VERSION"
echo "Build Tools: $LATEST_BUILD_TOOLS"
echo

echo "🧹 Cleaning previous build..."
rm -rf "target/dx/$PROJECT_NAME/release/android"

dx build --platform android --release

echo "🔧 Copying custom icons..."
RES_DIR="target/dx/$PROJECT_NAME/release/android/app/app/src/main/res"
find "$RES_DIR" -name "ic_launcher.webp" -delete
find "$RES_DIR" -name "ic_launcher_round.webp" -delete
# Remove adaptive icon XMLs — they override PNGs on Android 8+ and show the default robot icon
rm -f "$RES_DIR/mipmap-anydpi-v26/ic_launcher.xml"
rm -f "$RES_DIR/mipmap-anydpi-v26/ic_launcher_round.xml"
cp -r manual_assets/android_icons/mipmap-* "$RES_DIR/"
echo "✅ Icons copied successfully."

BUILD_DIR="target/dx/$PROJECT_NAME/release/android/app"
BUILD_GRADLE="$BUILD_DIR/app/build.gradle.kts"
OLD_PACKAGE="{old_package}"
NEW_PACKAGE="{android_package}"

if grep -q "$OLD_PACKAGE" "$BUILD_GRADLE"; then
    echo "⚠️  Incorrect package name detected ($OLD_PACKAGE)."
    echo "🔧 Applying automatic fix to set package to: $NEW_PACKAGE"
    sed -i '' "s/$OLD_PACKAGE/$NEW_PACKAGE/g" "$BUILD_GRADLE"
    find "$BUILD_DIR/app/src" -name "*.kt" -type f -exec sed -i '' "s/$OLD_PACKAGE/$NEW_PACKAGE/g" {{}} \;
    find "$BUILD_DIR/app/src" -name "AndroidManifest.xml" -type f -exec sed -i '' "s/$OLD_PACKAGE/$NEW_PACKAGE/g" {{}} \;
else
    echo "✅ Package name appears correct."
fi

echo "🔧 Ensuring App Name is '{app_name}'..."
find "$BUILD_DIR/app/src" -name "AndroidManifest.xml" -type f -exec sed -i '' 's/android:label="@string\/app_name"/android:label="{app_name}"/g' {{}} \;

echo "↻ Rebuilding with Gradle..."
CURRENT_DIR=$(pwd)
cd "$BUILD_DIR"
./gradlew clean assembleDebug --configuration-cache
cd "$CURRENT_DIR"

echo
echo "✅ Gradle Build complete"

GENERATED_APK="$(find target/dx -name "*.apk" -type f -exec ls -t {{}} + 2>/dev/null | head -1)"
ALIGNED_APK="$(dirname "$GENERATED_APK")/app-aligned.apk"
SIGNED_APK="$(dirname "$GENERATED_APK")/{project_slug}-signed.apk"

if [[ -f "$GENERATED_APK" ]]; then
    echo "🔧 Optimizing APK..."
    echo "  > Aligning..."
    rm -f "$ALIGNED_APK"
    "$ZIPALIGN" -v -p 4 "$GENERATED_APK" "$ALIGNED_APK" > /dev/null
    echo "  > Signing (v1 + v2)..."
    "$APKSIGNER" sign --ks "$HOME/.android/debug.keystore" \
                      --ks-pass pass:android \
                      --key-pass pass:android \
                      --out "$SIGNED_APK" \
                      "$ALIGNED_APK"
    echo "  > Verifying..."
    "$APKSIGNER" verify "$SIGNED_APK"
    rm "$ALIGNED_APK"
    echo "✅ Signed APK created: $SIGNED_APK"
    touch "$SIGNED_APK"
fi

if [[ -f "$SIGNED_APK" ]]; then
    TARGET_APK="./{project_slug}-debug.apk"
    cp "$SIGNED_APK" "$TARGET_APK"
    echo "📦 APK copied to: $TARGET_APK"
    open -R "$TARGET_APK"
    open "$(pwd)"
else
    APK_PATH="$(find target/dx -name "*.apk" -type f -exec ls -t {{}} + 2>/dev/null | head -1)"
    if [[ -n "$APK_PATH" ]]; then
        TARGET_APK="./{project_slug}-debug.apk"
        cp "$APK_PATH" "$TARGET_APK"
        echo "📦 APK copied to: $TARGET_APK"
        open -R "$TARGET_APK"
        open "$(pwd)"
    fi
fi
"##
    )
}

#[cfg(test)]
mod tests;
