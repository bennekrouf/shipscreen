//! Making the store images: generating, compositing, resizing and lettering
//! screenshots, and the icon.

use super::*;

pub(super) fn make_icon() -> Option<dioxus::desktop::tao::window::Icon> {
    const SIZE: u32 = 64;
    let mut rgba = Vec::with_capacity((SIZE * SIZE * 4) as usize);
    for y in 0..SIZE {
        for x in 0..SIZE {
            // Circular mask
            let cx = x as f32 - SIZE as f32 / 2.0 + 0.5;
            let cy = y as f32 - SIZE as f32 / 2.0 + 0.5;
            let r_sq = cx * cx + cy * cy;
            let radius = SIZE as f32 / 2.0;
            let alpha = if r_sq > (radius * radius) { 0u8 } else { 255u8 };

            // Gradient: top-left blue → bottom-right purple (ShipScreen identity)
            let t = (x + y) as f32 / (SIZE * 2) as f32;
            let r = (0.0_f32 + t * 120.0) as u8;
            let g = (120.0 - t * 60.0) as u8;
            let b = (212.0 - t * 20.0) as u8;

            // White phone-screens shape: two slightly-offset rounded rectangles
            // suggesting the App Store / Play Store dual export.
            let in_shape = is_screens_shape(x, y, SIZE);
            let (r, g, b) = if in_shape {
                (255u8, 255u8, 255u8)
            } else {
                (r, g, b)
            };

            rgba.extend_from_slice(&[r, g, b, alpha]);
        }
    }
    dioxus::desktop::tao::window::Icon::from_rgba(rgba, SIZE, SIZE).ok()
}

pub(super) fn is_screens_shape(x: u32, y: u32, size: u32) -> bool {
    let s = size as f32;
    let fx = x as f32;
    let fy = y as f32;

    // Front phone: rounded rect, slight tilt right
    let phone = |cx: f32, cy: f32, w: f32, h: f32, corner: f32| -> bool {
        let left = cx - w / 2.0;
        let right = cx + w / 2.0;
        let top = cy - h / 2.0;
        let bottom = cy + h / 2.0;
        if fx < left || fx > right || fy < top || fy > bottom {
            return false;
        }
        // Round the corners
        let dx = if fx < left + corner {
            left + corner - fx
        } else if fx > right - corner {
            fx - (right - corner)
        } else {
            0.0
        };
        let dy = if fy < top + corner {
            top + corner - fy
        } else if fy > bottom - corner {
            fy - (bottom - corner)
        } else {
            0.0
        };
        dx * dx + dy * dy <= corner * corner
    };

    // Back phone (offset up-left)
    if phone(s * 0.42, s * 0.45, s * 0.32, s * 0.48, s * 0.05) {
        return true;
    }
    // Front phone (offset down-right)
    if phone(s * 0.58, s * 0.55, s * 0.32, s * 0.48, s * 0.05) {
        return true;
    }
    false
}

pub(super) fn build_prompt(
    user_style: &str,
    phone_style: &str,
    screen_index: usize,
    total_screens: usize,
) -> String {
    let consistency_note = if total_screens > 1 {
        format!(
            " This is frame {screen_index} of {total_screens} in a set. \
             ALL frames MUST share the exact same visual theme: same background style, \
             same color palette, same decorative elements, same lighting and mood."
        )
    } else {
        String::new()
    };

    format!(
        "Generate a beautiful app store screenshot presentation frame. \
         Show a {phone_style} device in a front-facing view, centered in the image. \
         The device screen area must be filled with a perfectly uniform solid grey color \
         exactly #808080 with absolutely no texture, gradient, reflections, or content inside it — \
         just a flat solid grey rectangle with crisp sharp edges. \
         The background around the device should be themed in the style of: {user_style}. \
         Make the background beautiful, decorative, and eye-catching. \
         The device frame/bezel should look realistic and premium. \
         High quality, photorealistic, app store marketing material.{consistency_note}"
    )
}

pub(super) async fn call_fal_text_to_image(
    api_key: &str,
    prompt: &str,
    width: u32,
    height: u32,
    num_inference_steps: u32,
) -> Result<String, String> {
    let body = FalTextToImageRequest {
        prompt: prompt.to_string(),
        image_size: FalImageSize { width, height },
        num_inference_steps,
        guidance_scale: 3.5,
        num_images: 1,
        output_format: "png".to_string(),
    };
    let resp = reqwest::Client::new()
        .post("https://fal.run/fal-ai/flux/dev")
        .header("Authorization", format!("Key {api_key}"))
        .header("Content-Type", "application/json")
        .json(&body)
        .send()
        .await
        .map_err(|e| format!("HTTP request failed: {e}"))?;
    if !resp.status().is_success() {
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        return Err(format!("API returned {status}: {text}"));
    }
    let fal_resp: FalResponse = resp
        .json()
        .await
        .map_err(|e| format!("Failed to parse response: {e}"))?;
    fal_resp
        .images
        .first()
        .map(|i| i.url.clone())
        .ok_or_else(|| "No images in response".to_string())
}

pub(super) fn find_placeholder_rect(frame: &RgbaImage) -> Option<(u32, u32, u32, u32)> {
    let (fw, fh) = frame.dimensions();
    let is_grey = |x: u32, y: u32| -> bool {
        let p = frame.get_pixel(x, y);
        let (r, g, b) = (p[0] as i32, p[1] as i32, p[2] as i32);
        let max_diff = (r - g).abs().max((r - b).abs()).max((g - b).abs());
        let avg = (r + g + b) / 3;
        max_diff < 30 && avg > 90 && avg < 170
    };
    let mut min_x = fw;
    let mut max_x = 0u32;
    let mut min_y = fh;
    let mut max_y = 0u32;
    let step = 2u32;
    for y in (0..fh).step_by(step as usize) {
        for x in (0..fw).step_by(step as usize) {
            if is_grey(x, y) {
                min_x = min_x.min(x);
                max_x = max_x.max(x);
                min_y = min_y.min(y);
                max_y = max_y.max(y);
            }
        }
    }
    if max_x <= min_x || max_y <= min_y {
        return None;
    }
    let threshold = 0.7;
    for y in min_y..=max_y {
        let gc = (min_x..=max_x)
            .step_by(step as usize)
            .filter(|&x| is_grey(x, y))
            .count();
        let tot = ((max_x - min_x) / step + 1) as usize;
        if (gc as f64 / tot as f64) >= threshold {
            min_y = y;
            break;
        }
    }
    for y in (min_y..=max_y).rev() {
        let gc = (min_x..=max_x)
            .step_by(step as usize)
            .filter(|&x| is_grey(x, y))
            .count();
        let tot = ((max_x - min_x) / step + 1) as usize;
        if (gc as f64 / tot as f64) >= threshold {
            max_y = y;
            break;
        }
    }
    for x in min_x..=max_x {
        let gc = (min_y..=max_y)
            .step_by(step as usize)
            .filter(|&y| is_grey(x, y))
            .count();
        let tot = ((max_y - min_y) / step + 1) as usize;
        if (gc as f64 / tot as f64) >= threshold {
            min_x = x;
            break;
        }
    }
    for x in (min_x..=max_x).rev() {
        let gc = (min_y..=max_y)
            .step_by(step as usize)
            .filter(|&y| is_grey(x, y))
            .count();
        let tot = ((max_y - min_y) / step + 1) as usize;
        if (gc as f64 / tot as f64) >= threshold {
            max_x = x;
            break;
        }
    }
    let w = max_x.saturating_sub(min_x);
    let h = max_y.saturating_sub(min_y);
    let area = w as u64 * h as u64;
    let frame_area = fw as u64 * fh as u64;
    if area > frame_area / 20 && w > 50 && h > 50 {
        Some((min_x, min_y, w, h))
    } else {
        None
    }
}

pub(super) fn composite_screenshot(
    frame: &mut RgbaImage,
    screenshot: &RgbaImage,
    rect: (u32, u32, u32, u32),
) {
    let (rx, ry, rw, rh) = rect;
    let resized = image::imageops::resize(screenshot, rw, rh, FilterType::Lanczos3);
    let mut saved: Vec<(u32, u32, image::Rgba<u8>)> = Vec::new();
    for sy in 0..rh {
        for sx in 0..rw {
            let (fx, fy) = (rx + sx, ry + sy);
            if fx < frame.width() && fy < frame.height() {
                let fp = *frame.get_pixel(fx, fy);
                let (r, g, b) = (fp[0] as i32, fp[1] as i32, fp[2] as i32);
                let max_diff = (r - g).abs().max((r - b).abs()).max((g - b).abs());
                let avg = (r + g + b) / 3;
                if !(max_diff < 30 && avg > 90 && avg < 170) {
                    saved.push((fx, fy, fp));
                }
            }
        }
    }
    image::imageops::overlay(frame, &resized, rx as i64, ry as i64);
    for (fx, fy, pixel) in saved {
        frame.put_pixel(fx, fy, pixel);
    }
}

impl ExportTargets {
    pub(super) fn for_project(project_dir: &std::path::Path, p: &ProjectState) -> Self {
        let fastlane = project_dir.join("fastlane");
        Self {
            ios_dir: fastlane.join("screenshots").join("ios"),
            android_root: fastlane.join("metadata").join("android"),
            desktop_dir: fastlane.join("screenshots").join("desktop"),
            ios: if p.export_ios {
                p.ios_targets.clone()
            } else {
                vec![]
            },
            android: if p.export_android {
                p.android_targets.clone()
            } else {
                vec![]
            },
            desktop: if p.platform_type.has_desktop() {
                p.desktop_targets.clone()
            } else {
                vec![]
            },
        }
    }
}

/// Resize one composited screen to every enabled target and save it.
///
/// iOS goes to `fastlane/screenshots/ios/<locale>/`, desktop to
/// `fastlane/screenshots/desktop/<locale>/`, Android to fastlane supply's
/// layout: `<play-lang>/images/phoneScreenshots/NN.png` and, from the first
/// screen only (Play shows one), `<play-lang>/images/featureGraphic.png`.
/// Every label carries `[<locale>]` so uploads can pick each language's files.
pub(super) fn resize_to_targets(
    image_bytes: &[u8],
    screen_index: usize,
    locale: &str,
    play_locale: &str,
    targets: &ExportTargets,
) -> Result<Vec<(String, PathBuf)>, String> {
    let img = image::load_from_memory(image_bytes)
        .map_err(|e| format!("Failed to decode image: {e}"))?
        .to_rgba8();

    let mut results = Vec::new();

    for (ti, &(_, device_name, tw, th)) in IOS_TARGETS.iter().enumerate() {
        if !targets.ios.get(ti).copied().unwrap_or(false) {
            continue;
        }
        let resized = fill_and_crop(&img, tw, th);
        let locale_dir = targets.ios_dir.join(locale);
        std::fs::create_dir_all(&locale_dir)
            .map_err(|e| format!("Failed to create locale dir: {e}"))?;
        let path = locale_dir.join(format!("{device_name}-{screen_index:02}.png"));
        // App Store Connect rejects PNGs with an alpha channel (IMAGE_ALPHA_NOT_ALLOWED).
        // Flatten alpha onto white before saving iOS screenshots.
        flatten_alpha_onto_white(&resized)
            .save(&path)
            .map_err(|e| format!("Failed to save {}: {e}", path.display()))?;
        results.push((
            format!("iOS {device_name} [{locale}] #{screen_index}"),
            path,
        ));
    }

    let images_dir = targets.android_root.join(play_locale).join("images");
    for (ti, &(key, label, tw, th)) in ANDROID_TARGETS.iter().enumerate() {
        if !targets.android.get(ti).copied().unwrap_or(false) {
            continue;
        }
        let path = match key {
            "android_feature" if screen_index == 1 => images_dir.join("featureGraphic.png"),
            "android_feature" => continue,
            _ => images_dir
                .join("phoneScreenshots")
                .join(format!("{screen_index:02}.png")),
        };
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)
                .map_err(|e| format!("Failed to create {}: {e}", dir.display()))?;
        }
        fill_and_crop(&img, tw, th)
            .save(&path)
            .map_err(|e| format!("Failed to save {}: {e}", path.display()))?;
        results.push((format!("{label} [{locale}] #{screen_index}"), path));
    }

    for (ti, &(key, name, tw, th)) in DESKTOP_TARGETS.iter().enumerate() {
        if !targets.desktop.get(ti).copied().unwrap_or(false) {
            continue;
        }
        let locale_dir = targets.desktop_dir.join(locale);
        std::fs::create_dir_all(&locale_dir)
            .map_err(|e| format!("Failed to create {}: {e}", locale_dir.display()))?;
        let path = locale_dir.join(format!("{key}-{screen_index:02}.png"));
        // The Mac App Store refuses alpha just like iOS does.
        flatten_alpha_onto_white(&fill_and_crop(&img, tw, th))
            .save(&path)
            .map_err(|e| format!("Failed to save {}: {e}", path.display()))?;
        results.push((format!("Desktop {name} [{locale}] #{screen_index}"), path));
    }

    Ok(results)
}

pub(super) fn draw_centered_text(
    img: &mut RgbaImage,
    font: &FontRef,
    text: &str,
    scale: PxScale,
    color: Rgba<u8>,
    y_pos: i32,
) {
    let (w, _) = img.dimensions();
    let (text_width, _) = text_size(scale, font, text);
    draw_text_mut(
        img,
        color,
        (w as i32 - text_width as i32) / 2,
        y_pos,
        scale,
        font,
        text,
    );
}
