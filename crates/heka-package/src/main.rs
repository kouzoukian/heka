use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};
use std::{env, fs};

use heka_package::release::{Release, check_tag};
use heka_package::{Error, binary, crates};

const VERSION: &str = env!("CARGO_PKG_VERSION");
const TARGET: &str = "x86_64-pc-windows-msvc";

fn main() -> ExitCode {
    match package(&env::args().skip(1).collect::<Vec<_>>()) {
        Ok(zip) => {
            println!("{}", zip.display());
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("heka-package: {error}");
            ExitCode::FAILURE
        }
    }
}

fn package(args: &[String]) -> Result<PathBuf, Error> {
    match args {
        [] => {}
        [flag, tag] if flag == "--tag" => check_tag(tag, VERSION)?,
        _ => return Err("usage: heka-package [--tag v<version>]".into()),
    }
    let workspace = Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .ok_or("cannot find the top of the repository")?;
    let cargo = env::var_os("CARGO").unwrap_or_else(|| "cargo".into());
    let run = |program: &std::ffi::OsStr, args: &[&str]| -> Result<String, Error> {
        let output = Command::new(program)
            .args(args)
            .current_dir(workspace)
            .stderr(std::process::Stdio::inherit())
            .output()?;
        if !output.status.success() {
            return Err(format!("{program:?} {args:?} failed: {}", output.status).into());
        }
        Ok(String::from_utf8(output.stdout)?)
    };

    run(
        &cargo,
        &["build", "--release", "--locked", "-p", "heka-app"],
    )?;
    let metadata = run(
        &cargo,
        &[
            "metadata",
            "--format-version=1",
            "--locked",
            "--filter-platform",
            TARGET,
        ],
    )?;
    let target = serde_json::from_str::<serde_json::Value>(&metadata)?["target_directory"]
        .as_str()
        .map(PathBuf::from)
        .ok_or("cargo metadata names no target directory")?;
    let exe = target.join("release").join("heka.exe");
    let bytes =
        fs::read(&exe).map_err(|error| format!("cannot read {}: {error}", exe.display()))?;
    let imports = binary::check(&bytes)?;
    eprintln!("heka.exe imports {}", imports.join(", "));

    let sysroot = run("rustc".as_ref(), &["--print", "sysroot"])?;
    let release = Release {
        version: VERSION,
        workspace,
        exe: &exe,
        rust_notices: &Path::new(sysroot.trim()).join("share/doc/rust/COPYRIGHT-library.html"),
        crates: &crates::shipped(&metadata, "heka-app")?,
    };
    release.zip(&target.join("dist"))
}
