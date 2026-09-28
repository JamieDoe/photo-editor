use serde_json::{Value, json};

pub fn build(results: &[Value], iterations: usize) -> Value {
    json!({
        "phase": 0,
        "iterations": iterations,
        "environment": environment(),
        "renderer_version": renderer::RENDERER_VERSION,
        "results": results,
    })
}

fn environment() -> Value {
    let cpu = if cfg!(target_os = "macos") {
        std::process::Command::new("sysctl")
            .args(["-n", "machdep.cpu.brand_string"])
            .output()
            .ok()
            .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_owned())
    } else {
        None
    };
    json!({
        "os": std::env::consts::OS,
        "arch": std::env::consts::ARCH,
        "cpu": cpu,
        "threads": std::thread::available_parallelism().map_or(0, |n| n.get()),
        "libraw": raw::LibRawDecoder::libraw_version(),
        "profile": if cfg!(debug_assertions) { "debug" } else { "release" },
    })
}

pub fn markdown(report: &Value) -> String {
    let f = |v: &Value| v.as_f64().map_or("—".into(), |x| format!("{x:.1}"));
    let mut out = String::new();
    let env = &report["environment"];
    out.push_str(&format!(
        "Environment: {} · {} threads · {} {} · LibRaw {} · {} build\n\n",
        env["cpu"].as_str().unwrap_or("unknown CPU"),
        env["threads"],
        env["os"].as_str().unwrap_or(""),
        env["arch"].as_str().unwrap_or(""),
        env["libraw"].as_str().unwrap_or("?"),
        env["profile"].as_str().unwrap_or("?"),
    ));
    out.push_str("| File | MP | Preview decode (ms) | Pyramid (ms) | Thumb render (ms) | Interactive render (ms) | Detail render (ms) | Full decode (ms) | Full render (ms) | Export total (ms) | App peak RSS (MB) |\n");
    out.push_str("|---|---|---|---|---|---|---|---|---|---|---|\n");
    for r in report["results"].as_array().into_iter().flatten() {
        if let Some(err) = r.get("error") {
            out.push_str(&format!(
                "| {} | error: {} |||||||||\n",
                r["file"].as_str().unwrap_or("?"),
                err
            ));
            continue;
        }
        let p = &r["previews"];
        out.push_str(&format!(
            "| {} | {} | {} | {} | {} ({}) | {} ({}) | {} ({}) | {} | {} | {} | {} |\n",
            r["file"].as_str().unwrap_or("?"),
            f(&r["megapixels"]),
            f(&r["decode_preview_ms"]),
            f(&r["pyramid_ms"]),
            f(&p["thumbnail"]["render_ms"]),
            p["thumbnail"]["size"].as_str().unwrap_or(""),
            f(&p["interactive"]["render_ms"]),
            p["interactive"]["size"].as_str().unwrap_or(""),
            f(&p["detail"]["render_ms"]),
            p["detail"]["size"].as_str().unwrap_or(""),
            f(&r["decode_full_ms"]),
            f(&r["render_full_ms"]),
            f(&r["export"]["total_ms"]),
            f(&r["app_memory"]["peak_after_export_mb"]),
        ));
    }
    out.push_str("\n| File | Export: decode / render / encode / write (ms) | Cancel latency p50 / max (ms) | Interactive p50 idle → during export (ms) | App peak RSS open / previews / export (MB) |\n|---|---|---|---|---|\n");
    for r in report["results"].as_array().into_iter().flatten() {
        if r.get("error").is_some() {
            continue;
        }
        let e = &r["export"];
        let u = &r["interactive_under_export"];
        let m = &r["app_memory"];
        out.push_str(&format!(
            "| {} | {} / {} / {} / {} | {} / {} | {} → {} | {} / {} / {} |\n",
            r["file"].as_str().unwrap_or("?"),
            f(&e["decode_ms"]),
            f(&e["render_ms"]),
            f(&e["encode_ms"]),
            f(&e["write_ms"]),
            f(&r["cancel_latency_ms"]["p50"]),
            f(&r["cancel_latency_ms"]["max"]),
            f(&u["idle_p50_ms"]),
            f(&u["during_export_p50_ms"]),
            f(&m["peak_after_open_mb"]),
            f(&m["peak_after_previews_mb"]),
            f(&m["peak_after_export_mb"]),
        ));
    }
    out.push_str("\nEmbedded preview extraction (warm), ms:\n\n| File | ms | embedded → shown |\n|---|---|---|\n");
    for r in report["results"].as_array().into_iter().flatten() {
        if r.get("error").is_some() {
            continue;
        }
        let e = &r["embedded_preview"];
        let row = if e.is_null() {
            "— | none".to_owned()
        } else {
            format!(
                "{} | {} → {}",
                f(&e["ms"]),
                e["embedded_size"].as_str().unwrap_or(""),
                e["shown_size"].as_str().unwrap_or("")
            )
        };
        out.push_str(&format!(
            "| {} | {} |\n",
            r["file"].as_str().unwrap_or("?"),
            row
        ));
    }
    out.push_str("\nFull-resolution JPEG encode (q92, 4:4:4), ms / MB:\n\n| File | jpeg-encoder | libjpeg-turbo |\n|---|---|---|\n");
    for r in report["results"].as_array().into_iter().flatten() {
        let e = &r["jpeg_encoders"];
        if e.is_null() {
            continue;
        }
        let cell = |k: &str| {
            let v = &e[k];
            if v.is_null() {
                "—".to_owned()
            } else {
                format!(
                    "{} / {:.1}",
                    f(&v["ms"]),
                    v["bytes"].as_f64().unwrap_or(0.0) / 1e6
                )
            }
        };
        out.push_str(&format!(
            "| {} | {} | {} |\n",
            r["file"].as_str().unwrap_or("?"),
            cell("jpeg-encoder"),
            cell("libjpeg-turbo")
        ));
    }
    out.push_str(
        "\nPer-stage cost on the interactive level (cumulative, ms):\n\n| File | Level | ",
    );
    let first = report["results"]
        .as_array()
        .and_then(|a| a.iter().find(|r| r.get("error").is_none()));
    let labels: Vec<String> = first
        .and_then(|r| r["stage_costs_interactive"].as_array())
        .map(|a| {
            a.iter()
                .map(|s| s["stages"].as_str().unwrap_or("?").to_owned())
                .collect()
        })
        .unwrap_or_default();
    out.push_str(&labels.join(" | "));
    out.push_str(" |\n|---|---|");
    out.push_str(&"---|".repeat(labels.len()));
    out.push('\n');
    for r in report["results"].as_array().into_iter().flatten() {
        let Some(stages) = r["stage_costs_interactive"].as_array() else {
            continue;
        };
        let cells: Vec<String> = stages.iter().map(|s| f(&s["ms"])).collect();
        out.push_str(&format!(
            "| {} | {} | {} |\n",
            r["file"].as_str().unwrap_or("?"),
            r["interactive_level"].as_str().unwrap_or(""),
            cells.join(" | ")
        ));
    }
    out
}
