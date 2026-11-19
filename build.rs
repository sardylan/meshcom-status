use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

const WEB: &str = "web";
const SKIP: &str = "MESHCOM_STATUS_SKIP_WEB";
const NPM: &str = if cfg!(windows) { "npm.cmd" } else { "npm" };

const PLACEHOLDER: &str = r#"<!doctype html>
<html lang="en">
<head><meta charset="utf-8"><title>MeshCom Status</title>
<script>window.PAGE_CONFIG = /*PAGE_CONFIG*/null;</script></head>
<body><p>This build has no web front end (built with MESHCOM_STATUS_SKIP_WEB=1). The API is available.</p></body>
</html>
"#;

fn fail(message: String) -> ! {
    panic!("{message}");
}

fn brotli(data: &[u8]) -> Vec<u8> {
    let params = brotli::enc::BrotliEncoderParams {
        quality: 11,
        lgwin: 24,
        size_hint: data.len(),
        ..Default::default()
    };
    let mut out = Vec::new();
    brotli::BrotliCompress(&mut &data[..], &mut out, &params)
        .unwrap_or_else(|e| fail(format!("cannot compress with Brotli: {e}")));
    out
}

fn write_compressed(path: &Path, data: &[u8]) {
    fs::write(path, brotli(data)).unwrap_or_else(|e| fail(format!("cannot write {}: {e}", path.display())));
}

fn copy_dir(from: &Path, to: &Path) {
    fs::create_dir_all(to).unwrap_or_else(|e| fail(format!("cannot create {}: {e}", to.display())));
    let entries = fs::read_dir(from).unwrap_or_else(|e| fail(format!("cannot read {}: {e}", from.display())));
    for entry in entries {
        let entry = entry.unwrap_or_else(|e| fail(format!("cannot read {}: {e}", from.display())));
        let target = to.join(entry.file_name());
        if entry.path().is_dir() {
            copy_dir(&entry.path(), &target);
        } else {
            let data =
                fs::read(entry.path()).unwrap_or_else(|e| fail(format!("cannot read {}: {e}", entry.path().display())));
            write_compressed(&target, &data);
        }
    }
}

fn npm(web: &Path, args: &[&str]) {
    let output = Command::new(NPM)
        .args(args)
        .current_dir(web)
        .output()
        .unwrap_or_else(|e| {
            fail(format!(
                "cannot run npm (is Node.js on PATH? run `nvm use` in {WEB}/, or set {SKIP}=1): {e}"
            ))
        });
    if !output.status.success() {
        fail(format!(
            "`npm {}` failed in {WEB}/:\n{}\n{}",
            args.join(" "),
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        ));
    }
}

fn modified(path: &Path) -> Option<std::time::SystemTime> {
    fs::metadata(path).and_then(|m| m.modified()).ok()
}

fn check_node_version(web: &Path) {
    let wanted = fs::read_to_string(web.join(".nvmrc")).unwrap_or_default();
    let wanted = wanted.trim().trim_start_matches('v');
    let found = Command::new("node").arg("--version").output().ok().map(|o| {
        String::from_utf8_lossy(&o.stdout)
            .trim()
            .trim_start_matches('v')
            .to_owned()
    });
    match found {
        Some(v) if v == wanted => {}
        Some(v) => {
            println!("cargo:warning=Node.js {v} on PATH, but {WEB}/.nvmrc asks for {wanted}: run `nvm use` in {WEB}/")
        }
        None => println!("cargo:warning=node not found on PATH"),
    }
}

fn web_versions(web: &Path) -> Vec<(String, Option<String>)> {
    let read = |file: &str| -> serde_json::Value {
        let path = web.join(file);
        let text = fs::read_to_string(&path).unwrap_or_else(|e| fail(format!("cannot read {}: {e}", path.display())));
        serde_json::from_str(&text).unwrap_or_else(|e| fail(format!("cannot parse {}: {e}", path.display())))
    };
    let text = |v: &serde_json::Value| v.as_str().map(str::to_owned);
    let package = read("package.json");
    let lock = read("package-lock.json");
    vec![
        (format!("{WEB}/package.json version"), text(&package["version"])),
        (format!("{WEB}/package-lock.json version"), text(&lock["version"])),
        (
            format!("{WEB}/package-lock.json packages[\"\"].version"),
            text(&lock["packages"][""]["version"]),
        ),
    ]
}

fn check_versions(web: &Path) {
    let crate_version =
        env::var("CARGO_PKG_VERSION").unwrap_or_else(|e| fail(format!("CARGO_PKG_VERSION not set: {e}")));
    let wrong: Vec<String> = web_versions(web)
        .into_iter()
        .filter(|(_, v)| v.as_deref() != Some(crate_version.as_str()))
        .map(|(what, v)| format!("{what} is {}", v.as_deref().unwrap_or("missing")))
        .collect();
    if !wrong.is_empty() {
        fail(format!(
            "the front end must have the version of Cargo.toml ({crate_version}): {}. Set it with `npm version {crate_version} --no-git-tag-version` in {WEB}/.",
            wrong.join(", ")
        ));
    }
}

fn main() {
    println!("cargo:rerun-if-env-changed={SKIP}");
    println!("cargo:rerun-if-changed={WEB}/package.json");
    println!("cargo:rerun-if-changed={WEB}/package-lock.json");
    check_versions(Path::new(WEB));
    let out = PathBuf::from(env::var("OUT_DIR").unwrap_or_else(|e| fail(format!("OUT_DIR not set: {e}")))).join("web");
    if out.exists() {
        fs::remove_dir_all(&out).unwrap_or_else(|e| fail(format!("cannot clean {}: {e}", out.display())));
    }
    fs::create_dir_all(&out).unwrap_or_else(|e| fail(format!("cannot create {}: {e}", out.display())));

    if env::var(SKIP).is_ok_and(|v| v == "1") {
        println!("cargo:warning={SKIP}=1: the web front end is not built, a placeholder page is embedded");
        write_compressed(&out.join("index.html"), PLACEHOLDER.as_bytes());
        return;
    }

    let web = PathBuf::from(WEB);
    for watched in [
        "src",
        "index.html",
        "package.json",
        "package-lock.json",
        ".nvmrc",
        "vite.config.ts",
        "tsconfig.json",
        "tsconfig.app.json",
        "tsconfig.node.json",
    ] {
        println!("cargo:rerun-if-changed={WEB}/{watched}");
    }
    check_node_version(&web);

    let lock = modified(&web.join("package-lock.json"));
    let installed = modified(&web.join("node_modules/.package-lock.json"));
    if installed.is_none() || lock > installed {
        npm(&web, &["ci", "--no-audit", "--no-fund"]);
    }
    npm(&web, &["run", "build"]);
    copy_dir(&web.join("dist"), &out);
}
