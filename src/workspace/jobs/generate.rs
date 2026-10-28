//! Generating the store screenshots.

use super::*;

pub(crate) fn generate_ai(ws: Ws) {
    let mut proj = ws.proj;
    let settings = ws.settings;
    let mut phase = ws.gen_phase;
    let mut log_lines = ws.gen_log;
    let mut add_log = move |msg: String| {
        log_lines.write().push(msg);
    };
    let proj_dir = ws.dir();

    let prompt = proj.read().theme_prompt.clone();
    let selected_locales = proj.read().locales.clone();
    // Per-locale sources: locale → Vec<PathBuf>
    let locale_srcs: std::collections::HashMap<String, Vec<PathBuf>> = selected_locales
        .iter()
        .map(|loc| (loc.clone(), proj.read().sources_for(loc)))
        .collect();
    let targets = ExportTargets::for_project(&proj_dir, &proj.read());
    // Desktop screens are landscape; phone and tablet ones are portrait.
    let desktop = proj.read().platform_type.has_desktop();

    // Check every locale has at least one image
    let empty_locales: Vec<&String> = selected_locales
        .iter()
        .filter(|loc| locale_srcs.get(*loc).map(|v| v.is_empty()).unwrap_or(true))
        .collect();
    if !empty_locales.is_empty() {
        phase.set(AppPhase::Error(format!(
            "No images for locale(s): {}. Add images in each language tab.",
            empty_locales
                .iter()
                .map(|l| l.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        )));
        return;
    }
    if prompt.trim().is_empty() {
        phase.set(AppPhase::Error(
            "Please enter a theme prompt for AI generation.".into(),
        ));
        return;
    }

    // Save prompt to history
    {
        let trimmed = prompt.trim().to_string();
        let mut p = proj.write();
        p.theme_history.retain(|h| h != &trimmed);
        p.theme_history.insert(0, trimmed);
        p.theme_history.truncate(MAX_THEME_HISTORY);
        save_project_state(&proj_dir, &p);
    }

    let api_key = settings.read().fal_key.clone();
    let phone_style = settings.read().phone_style.clone();
    let steps_val = settings.read().inference_steps;
    let proj_dir = proj_dir.clone();

    ws.spawn(async move {
        phase.set(AppPhase::GeneratingAi);
        log_lines.set(Vec::new());
        ws.show_job(JobKind::Screenshots);
        {
            let mut p = proj.write();
            p.generated_urls.clear();
            p.output_paths.clear();
        }

        if api_key.is_empty() {
            phase.set(AppPhase::Error(
                "No fal.ai API key set. Add it in Settings (gear icon in the sidebar).".into(),
            ));
            return;
        }

        let (frame_w, frame_h): (u32, u32) = if desktop { (1920, 1200) } else { (1290, 2796) };
        let device = if desktop {
            "modern laptop computer".to_string()
        } else {
            phone_style.clone()
        };
        let mut all_outputs: Vec<(String, PathBuf)> = Vec::new();

        // Each locale has its own images — process independently
        for locale in &selected_locales {
            let srcs = locale_srcs.get(locale).cloned().unwrap_or_default();
            let total = srcs.len();
            // Drop last run's phone screenshots so fewer screens this time
            // doesn't leave old ones behind in the fastlane folder.
            if !targets.android.is_empty() {
                let _ = std::fs::remove_dir_all(
                    targets
                        .android_root
                        .join(stores::play_language(locale))
                        .join("images")
                        .join("phoneScreenshots"),
                );
            }
            if !targets.desktop.is_empty() {
                let _ = std::fs::remove_dir_all(targets.desktop_dir.join(locale));
            }
            add_log(format!(
                "─── AI generating locale: {locale} ({total} image(s)) ───"
            ));

            for (idx, src) in srcs.iter().enumerate() {
                let screen_num = idx + 1;
                add_log(format!(
                    "[{locale}] Reading image {screen_num}/{total}: {}",
                    src.file_name().unwrap_or_default().to_string_lossy()
                ));

                // Async file read — does not block the UI.
                let img_bytes = match tokio::fs::read(src).await {
                    Ok(b) => b,
                    Err(e) => {
                        phase.set(AppPhase::Error(format!(
                            "[{locale}] Failed to read image {screen_num}: {e}"
                        )));
                        return;
                    }
                };
                // Decode source screenshot (fast, fine on async thread).
                let screenshot = match image::load_from_memory(&img_bytes) {
                    Ok(img) => img.to_rgba8(),
                    Err(e) => {
                        phase.set(AppPhase::Error(format!(
                            "[{locale}] Failed to decode image {screen_num}: {e}"
                        )));
                        return;
                    }
                };

                let full_prompt = build_prompt(&prompt, &device, screen_num, total);
                add_log(format!(
                    "[{locale}] Generating frame {screen_num}/{total} via FLUX…"
                ));

                let frame_url = match call_fal_text_to_image(
                    &api_key,
                    &full_prompt,
                    frame_w,
                    frame_h,
                    steps_val,
                )
                .await
                {
                    Ok(url) => {
                        add_log(format!("Got frame {screen_num}"));
                        url
                    }
                    Err(e) => {
                        phase.set(AppPhase::Error(format!(
                            "[{locale}] Frame generation failed for screen {screen_num}: {e}"
                        )));
                        return;
                    }
                };

                add_log(format!("[{locale}] Downloading frame {screen_num}…"));
                let frame_bytes = match download_image(&frame_url).await {
                    Ok(b) => b,
                    Err(e) => {
                        phase.set(AppPhase::Error(format!(
                            "[{locale}] Download failed for frame {screen_num}: {e}"
                        )));
                        return;
                    }
                };

                // Heavy CPU work (decode AI frame, composite, encode, resize) on a
                // blocking thread so the async executor — and the UI — stay free.
                phase.set(AppPhase::Resizing);
                add_log(format!("[{locale}] Compositing screenshot {screen_num}…"));
                let locale2 = locale.clone();
                let targets2 = targets.clone();
                let play_locale = stores::play_language(locale).to_string();
                let cpu = tokio::task::spawn_blocking(move || -> Result<Composited, String> {
                    let mut frame_img = image::load_from_memory(&frame_bytes)
                        .map_err(|e| {
                            format!("[{locale2}] Failed to decode frame {screen_num}: {e}")
                        })?
                        .to_rgba8();
                    let rect = find_placeholder_rect(&frame_img).unwrap_or_else(|| {
                        fallback_placement(frame_img.width(), frame_img.height())
                    });
                    composite_screenshot(&mut frame_img, &screenshot, rect);

                    let composited_bytes = {
                        let mut buf = std::io::Cursor::new(Vec::new());
                        frame_img
                            .write_to(&mut buf, image::ImageFormat::Png)
                            .map_err(|e| {
                                format!("[{locale2}] Failed to encode image {screen_num}: {e}")
                            })?;
                        buf.into_inner()
                    };

                    let paths = resize_to_targets(
                        &composited_bytes,
                        screen_num,
                        &locale2,
                        &play_locale,
                        &targets2,
                    )
                    .map_err(|e| {
                        format!("Resize failed for [{locale2}] screen {screen_num}: {e}")
                    })?;

                    let preview = paths
                        .iter()
                        .find(|(l, _)| l.contains("6.7"))
                        .or_else(|| paths.first())
                        .map(|(_, p)| {
                            urlencoding::encode(p.to_string_lossy().as_ref()).into_owned()
                        });

                    Ok((paths, preview))
                });

                let (paths, preview) = match cpu.await {
                    Ok(Ok(v)) => v,
                    Ok(Err(e)) => {
                        phase.set(AppPhase::Error(e));
                        return;
                    }
                    Err(e) => {
                        phase.set(AppPhase::Error(format!("Thread panic: {e}")));
                        return;
                    }
                };

                for (label, _) in &paths {
                    add_log(format!("Saved: {label}"));
                }
                if let Some(enc) = preview {
                    let mut pw = proj.write();
                    if !pw.generated_urls.iter().any(|u| u.contains(&enc)) {
                        pw.generated_urls.push(format!("/localimg/{enc}"));
                    }
                }
                all_outputs.extend(paths);
                phase.set(AppPhase::GeneratingAi);
            }
        }

        {
            let mut p = proj.write();
            p.output_paths = all_outputs;
            save_project_state(&proj_dir, &p);
        }
        let loc_count = selected_locales.len();
        add_log(format!(
            "Done! AI screenshots generated for {loc_count} locale(s)."
        ));
        phase.set(AppPhase::Done);
    });
}

/// Manual screenshots for every selected language: each source screen on the
/// primary or secondary colour, under its title and subtitle, written to
/// every export size. `on_preview` gets each screen's preview (its 6.7″ copy,
/// URL-encoded) as soon as it is written. Returns every file written. The
/// window's Screenshots step and `mcp` both run this.
pub(crate) async fn generate_manual_images(
    proj_dir: PathBuf,
    p: ProjectState,
    log: Log,
    mut on_preview: impl FnMut(String),
) -> Result<Vec<(String, PathBuf)>, String> {
    if let Some(problem) = missing_sources(&p) {
        return Err(problem);
    }
    let targets = ExportTargets::for_project(&proj_dir, &p);
    // Desktop screens are landscape; phone and tablet ones are portrait.
    let desktop = p.platform_type.has_desktop();
    let primary_rgba = parse_hex_color(&p.primary_color).unwrap_or(Rgba([59, 130, 246, 255]));
    let secondary_rgba = if !p.secondary_color.is_empty() {
        parse_hex_color(&p.secondary_color).unwrap_or(lighten_color(primary_rgba))
    } else {
        lighten_color(primary_rgba)
    };
    let mut all_outputs: Vec<(String, PathBuf)> = Vec::new();

    // Each locale has its own images — process independently
    for locale in &p.locales {
        let srcs = p.sources_for(locale);
        let total = srcs.len();
        // Drop last run's phone screenshots so fewer screens this time
        // doesn't leave old ones behind in the fastlane folder.
        if !targets.android.is_empty() {
            let _ = std::fs::remove_dir_all(
                targets
                    .android_root
                    .join(stores::play_language(locale))
                    .join("images")
                    .join("phoneScreenshots"),
            );
        }
        if !targets.desktop.is_empty() {
            let _ = std::fs::remove_dir_all(targets.desktop_dir.join(locale));
        }
        let texts = p.texts_for(locale);
        log.push(format!(
            "─── Generating locale: {locale} ({total} image(s)) ───"
        ));

        for (idx, src) in srcs.iter().enumerate() {
            let screen_num = idx + 1;
            log.push(format!("[{locale}] Processing image {screen_num}/{total}…"));

            // Clone everything the blocking thread needs.
            let src2 = src.clone();
            let locale2 = locale.clone();
            let targets2 = targets.clone();
            let play_locale = stores::play_language(locale).to_string();
            let bg_color = if idx % 2 == 0 {
                primary_rgba
            } else {
                secondary_rgba
            };
            let (title, subtitle) = texts.get(idx).cloned().unwrap_or_default();

            let cpu = tokio::task::spawn_blocking(move || -> Result<Composited, String> {
                let (w, h): (u32, u32) = if desktop { (2560, 1600) } else { (1290, 2796) };
                let font = FontRef::try_from_slice(ROBOTO_FONT)
                    .map_err(|e| format!("The built-in font could not be read: {e}"))?;

                let screenshot_bytes = std::fs::read(&src2)
                    .map_err(|e| format!("Failed to read image {screen_num}: {e}"))?;
                let screenshot = image::load_from_memory(&screenshot_bytes)
                    .map_err(|e| format!("Failed to decode image {screen_num}: {e}"))?
                    .to_rgba8();

                let mut img = RgbaImage::from_pixel(w, h, bg_color);
                let text_color = get_contrast_color(bg_color);
                // (title size, title y, subtitle size, subtitle y)
                let (ts, ty, ss, sy) = if desktop {
                    (110.0, 90, 56.0, 230)
                } else {
                    (120.0, 200, 60.0, 350)
                };
                if !title.is_empty() {
                    draw_centered_text(&mut img, &font, &title, PxScale::from(ts), text_color, ty);
                }
                if !subtitle.is_empty() {
                    draw_centered_text(
                        &mut img,
                        &font,
                        &subtitle,
                        PxScale::from(ss),
                        text_color,
                        sy,
                    );
                }

                if desktop {
                    // Fit the whole window below the captions, keeping
                    // its proportions — a desktop screenshot can be any shape.
                    let (area_x, area_y) = (200u32, 360u32);
                    let (area_w, area_h) = (w - 2 * area_x, h - area_y - 120);
                    let scale = (area_w as f64 / screenshot.width() as f64)
                        .min(area_h as f64 / screenshot.height() as f64);
                    let fit_w = ((screenshot.width() as f64 * scale) as u32).max(1);
                    let fit_h = ((screenshot.height() as f64 * scale) as u32).max(1);
                    let resized =
                        image::imageops::resize(&screenshot, fit_w, fit_h, FilterType::Lanczos3);
                    image::imageops::overlay(
                        &mut img,
                        &resized,
                        (area_x + (area_w - fit_w) / 2) as i64,
                        (area_y + (area_h - fit_h) / 2) as i64,
                    );
                } else {
                    let phone_w = (w as f64 * 0.7) as u32;
                    let phone_h = (phone_w as f64 * 2.16) as u32;
                    let phone_x = (w - phone_w) / 2;
                    let phone_y = h - phone_h - 150;
                    let resized = image::imageops::resize(
                        &screenshot,
                        phone_w,
                        phone_h,
                        FilterType::Lanczos3,
                    );
                    image::imageops::overlay(&mut img, &resized, phone_x as i64, phone_y as i64);
                }

                let composited_bytes = {
                    let mut buf = std::io::Cursor::new(Vec::new());
                    img.write_to(&mut buf, image::ImageFormat::Png)
                        .map_err(|e| format!("Failed to encode image {screen_num}: {e}"))?;
                    buf.into_inner()
                };

                let paths = resize_to_targets(
                    &composited_bytes,
                    screen_num,
                    &locale2,
                    &play_locale,
                    &targets2,
                )
                .map_err(|e| format!("Resize failed for [{locale2}] screen {screen_num}: {e}"))?;

                let preview = paths
                    .iter()
                    .find(|(l, _)| l.contains("6.7"))
                    .or_else(|| paths.first())
                    .map(|(_, p)| urlencoding::encode(p.to_string_lossy().as_ref()).into_owned());

                Ok((paths, preview))
            });

            let (paths, preview) = match cpu.await {
                Ok(Ok(v)) => v,
                Ok(Err(e)) => return Err(e),
                Err(e) => return Err(format!("Thread panic: {e}")),
            };

            for (label, _) in &paths {
                log.push(format!("Saved: {label}"));
            }
            if let Some(enc) = preview {
                on_preview(enc);
            }
            all_outputs.extend(paths);
        }
    }
    log.push(format!(
        "Done! Manual screenshots generated for {} locale(s).",
        p.locales.len()
    ));
    Ok(all_outputs)
}

pub(crate) fn generate_manual(ws: Ws) {
    let mut proj = ws.proj;
    let mut phase = ws.gen_phase;
    let mut log_lines = ws.gen_log;
    let proj_dir = ws.dir();
    if let Some(problem) = missing_sources(&proj.read()) {
        phase.set(AppPhase::Error(problem));
        return;
    }

    ws.spawn(async move {
        phase.set(AppPhase::GeneratingManual);
        log_lines.set(Vec::new());
        ws.show_job(JobKind::Screenshots);
        let snapshot = {
            let mut p = proj.write();
            p.generated_urls.clear();
            p.output_paths.clear();
            p.clone()
        };
        let preview =
            move |enc: String| proj.write().generated_urls.push(format!("/localimg/{enc}"));
        match generate_manual_images(proj_dir.clone(), snapshot, log_lines.into(), preview).await {
            Ok(outputs) => {
                let mut p = proj.write();
                p.output_paths = outputs;
                save_project_state(&proj_dir, &p);
                phase.set(AppPhase::Done);
            }
            Err(e) => phase.set(AppPhase::Error(e)),
        }
    });
}
