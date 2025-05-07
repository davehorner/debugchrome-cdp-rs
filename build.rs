use std::env;
use std::fs;
use std::path::Path;

fn main() {
    // Get the current Git repository
    let repo = git2::Repository::discover(".").expect("Failed to find Git repository");

    // Get the short SHA of the current commit
    let head = repo.head().expect("Failed to get HEAD");
    let commit = head.peel_to_commit().expect("Failed to get commit");
    let short_sha = commit.id().to_string()[..7].to_string(); // Get the first 7 characters

    // Write the short SHA to an environment variable
    println!("cargo:rustc-env=GIT_SHORT_SHA={}", short_sha);

    // Optionally, write the short SHA to a file for debugging
    let out_dir = env::var("OUT_DIR").expect("OUT_DIR not set");
    let sha_file = Path::new(&out_dir).join("git_sha.txt");
    fs::write(sha_file, &short_sha).expect("Failed to write git_sha.txt");
}
