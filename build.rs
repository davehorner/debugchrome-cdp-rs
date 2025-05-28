// build.rs
use chrono::Datelike;
use git2::Repository;
use std::path::Path;
use std::{env, fs};

fn main() {
    if let Err(e) = try_main() {
        println!("Error in build.rs: {}", e);
        std::process::exit(1);
    }
}

fn try_main() -> Result<(), Box<dyn std::error::Error>> {
    println!("build.rs is running");

    let manifest = env::var("CARGO_MANIFEST_DIR")?;
    println!("CARGO_MANIFEST_DIR: {}", manifest);

    let last_release_path = Path::new(&manifest).join("LAST_RELEASE");
    println!("Looking for LAST_RELEASE at: {:?}", last_release_path);

    // Read LAST_RELEASE as raw bytes and handle invalid UTF-8
    let last = fs::read(&last_release_path)
        .map(|bytes| String::from_utf8_lossy(&bytes).trim().to_string())
        .unwrap_or_else(|err| {
            println!("Failed to read LAST_RELEASE: {}", err);
            "00/00/00|00000|0.0.0".into()
        });

    println!("LAST_RELEASE content: {}", last);

    // Split the LAST_RELEASE into its components
    let parts: Vec<&str> = last.split('|').collect();
    let (release_date, release_sha, release_version) = if parts.len() == 3 {
        (parts[0], parts[1], parts[2])
    } else {
        ("00/00/00", "00000", "0.0.0")
    };

    // Current SHA (5 hex) if in a git repo
    let current = Repository::discover(&manifest)
        .ok()
        .and_then(|repo| {
            let head = repo.head().ok()?;
            head.peel_to_commit().ok().map(|commit| commit.id())
        })
        .map(|id| id.to_string()[..5].to_string())
        .unwrap_or_else(|| "00000".to_string());

    // Build date DD/MM/YY
    let now = chrono::Local::now();
    let date = format!(
        "{:02}/{:02}/{:02}",
        now.day(),
        now.month(),
        now.year() % 100
    );

    // Emit environment variables for compile time
    println!("cargo:rerun-if-changed=LAST_RELEASE");
    println!("cargo:rustc-env=LAST_RELEASE={}", last);
    println!("cargo:rustc-env=LAST_RELEASE_DATE={}", release_date);
    println!("cargo:rustc-env=LAST_RELEASE_SHA={}", release_sha);
    println!("cargo:rustc-env=LAST_RELEASE_VERSION={}", release_version);
    println!("cargo:rustc-env=GIT_SHA_SHORT={}", current);
    println!("cargo:rustc-env=BUILD_DATE={}", date);

    Ok(())
}
