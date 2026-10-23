// SPDX-License-Identifier: Apache-2.0
//! Export the versioned native SDK and create/build independently usable C/C++/Rust projects.
//! Builds use an isolated user target, explicit ABI tags and locked local offline Rust dependencies.
use super::*;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

const RELEASE: &str = "anova-sdk-1.0.0";

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Project {
    schema: u32,
    name: String,
    language: String,
    version: u32,
}

pub fn dispatch(root: &Path, args: &[String]) -> Result<()> {
    match args {
        [] => build(root).map(|_| ()),
        [action] if action == "export" => build(root).map(|_| ()),
        _ => Err("Usage: cargo xtask sdk [export]".into()),
    }
}

/// Export only first-party source inputs and immutable version metadata, without host binaries.
pub fn build(root: &Path) -> Result<PathBuf> {
    let work = Temporary::new(root, "sdk-export")?;
    let destination = work.0.join("candidate");
    safe_directory(&destination)?;
    for (source, output) in [
        ("user/c/anova.h", "include/anova.h"),
        ("sdk/include/anova.hpp", "include/anova.hpp"),
        ("user/c/crt.S", "crt.S"),
        ("sdk/native-v1.ld", "native-v1.ld"),
        ("sdk/build.py", "build.py"),
        ("sdk/README.md", "README.md"),
        ("LICENSE", "LICENSE"),
        ("NOTICE", "NOTICE"),
    ] {
        copy_file(&root.join(source), &destination.join(output))?;
    }
    copy_tree(
        &root.join("crates/abi/src"),
        &destination.join("rust/abi/src"),
    )?;
    copy_tree(&root.join("user/rt/src"), &destination.join("rust/rt/src"))?;
    copy_tree(&root.join("sdk/templates"), &destination.join("templates"))?;
    write_file(
        &destination.join("rust/abi/Cargo.toml"),
        "# SPDX-License-Identifier: Apache-2.0\n# Standalone frozen native ABI SDK crate.\n[package]\nname = \"anova-abi\"\nversion = \"1.0.0\"\nedition = \"2024\"\nlicense = \"Apache-2.0\"\n",
    )?;
    write_file(
        &destination.join("rust/rt/Cargo.toml"),
        "# SPDX-License-Identifier: Apache-2.0\n# Standalone native SDK runtime with a local ABI dependency.\n[package]\nname = \"anova-rt\"\nversion = \"1.0.0\"\nedition = \"2024\"\nlicense = \"Apache-2.0\"\n\n[dependencies]\nanova-abi = { path = \"../abi\", version = \"=1.0.0\" }\n",
    )?;
    write_file(
        &destination.join("rust/Cargo.toml"),
        "# SPDX-License-Identifier: Apache-2.0\n# Independent native SDK source workspace.\n[workspace]\nmembers = [\"abi\", \"rt\"]\nresolver = \"3\"\n",
    )?;
    write_file(
        &destination.join("sdk.toml"),
        "# SPDX-License-Identifier: Apache-2.0\n# Released SDK identity; GUI remains a separately versioned future delivery.\nschema = 1\nversion = \"1.0.0\"\nabi = \"anova.native.v1\"\narchitecture = \"x86_64\"\nwire_version = 1\n\n[gui]\nreserved_abi = \"anova.gui.v1\"\nimplemented = false\n",
    )?;
    command(
        Command::new("cargo")
            .args(["generate-lockfile", "--offline", "--manifest-path"])
            .arg(destination.join("templates/rust/Cargo.toml")),
    )?;
    let mut files = Vec::new();
    collect_files(&destination, &destination, &mut files)?;
    files.sort();
    let mut checksums = String::new();
    for relative in files {
        if relative == Path::new("SHA256SUMS") {
            continue;
        }
        checksums.push_str(&format!(
            "{:x}  {}\n",
            Sha256::digest(fs::read(destination.join(&relative))?),
            relative.display()
        ));
    }
    write_file(&destination.join("SHA256SUMS"), &checksums)?;
    let published = root.join("build/sdk").join(RELEASE);
    safe_directory(published.parent().ok_or("SDK export has no parent")?)?;
    let backup = work.0.join("previous");
    if let Err(error) = publish_export(&destination, &published, &backup) {
        // Preserve the preceding release if an unexpected filesystem failure also prevents
        // restoring it. The diagnostic identifies this recovery location explicitly.
        if backup.exists() {
            core::mem::forget(work);
        }
        return Err(error);
    }
    println!("Exported native SDK 1.0.0: {}", published.display());
    Ok(published)
}

/// Reject unmanaged content rather than incorporating it into a new release or deleting it.
/// Known generated files can be replaced only after the complete fresh candidate exists.
fn validate_previous_export(candidate: &Path, previous: &Path) -> Result<()> {
    for entry in fs::read_dir(previous)? {
        let entry = entry?;
        let expected = candidate.join(entry.file_name());
        let metadata = fs::symlink_metadata(&expected).map_err(|error| {
            if error.kind() == std::io::ErrorKind::NotFound {
                format!(
                    "Unexpected SDK export content: {}; move it outside the generated release directory and rerun cargo xtask sdk",
                    entry.path().display()
                )
            } else {
                format!("Cannot inspect SDK export: {error}")
            }
        })?;
        let kind = entry.file_type()?;
        if kind.is_dir() && metadata.file_type().is_dir() {
            validate_previous_export(&expected, &entry.path())?;
        } else if !kind.is_file() || !metadata.file_type().is_file() {
            return Err(format!(
                "Refusing unexpected SDK export entry or symlink: {}; move it outside the generated release directory and rerun cargo xtask sdk",
                entry.path().display()
            )
            .into());
        }
    }
    Ok(())
}

pub(super) fn publish_export(candidate: &Path, destination: &Path, backup: &Path) -> Result<()> {
    let previous = match fs::symlink_metadata(destination) {
        Ok(metadata) if metadata.file_type().is_dir() => {
            validate_previous_export(candidate, destination)?;
            true
        }
        Ok(_) => {
            return Err(format!(
                "Refusing SDK export destination that is not a regular directory: {}",
                destination.display()
            )
            .into());
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => false,
        Err(error) => return Err(error.into()),
    };
    if previous {
        fs::rename(destination, backup)?;
    }
    if let Err(error) = fs::rename(candidate, destination) {
        if previous && let Err(restore) = fs::rename(backup, destination) {
            return Err(format!(
                "SDK publication failed ({error}) and restoration failed ({restore}); previous release remains at {}",
                backup.display()
            )
            .into());
        }
        return Err(error.into());
    }
    Ok(())
}

pub fn app_new(root: &Path, args: &[String]) -> Result<()> {
    let [language, path] = args else {
        return Err("Usage: cargo xtask app-new <c|cpp|rust> PROJECT".into());
    };
    if !matches!(language.as_str(), "c" | "cpp" | "rust") {
        return Err("Application language must be c, cpp or rust".into());
    }
    let project = project_path(root, path)?;
    if project.exists() || fs::symlink_metadata(&project).is_ok() {
        return Err(format!("Application project already exists: {}", project.display()).into());
    }
    let name = project
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or("Invalid project name")?;
    validate_name(name)?;
    let sdk = build(root)?;
    safe_directory(&project)?;
    copy_tree(&sdk, &project.join(".anova-sdk"))?;
    copy_tree(&sdk.join("templates").join(language), &project)?;
    let metadata = Project {
        schema: 1,
        name: name.to_owned(),
        language: language.clone(),
        version: 1,
    };
    write_file(
        &project.join("project.toml"),
        &format!(
            "# SPDX-License-Identifier: Apache-2.0\n# Application source identity and monotonic package upgrade version.\n{}",
            toml::to_string(&metadata)?
        ),
    )?;
    if language == "rust" {
        configure_rust_project(&project)?;
    }
    println!("Created {} application: {}", language, project.display());
    Ok(())
}

fn configure_rust_project(project: &Path) -> Result<()> {
    let manifest = project.join("Cargo.toml");
    let text = fs::read_to_string(&manifest)?;
    write_file(
        &manifest,
        &text.replace("path = \"../../rust/rt\"", "path = \".anova-sdk/rust/rt\""),
    )?;
    command(
        Command::new("cargo")
            .args(["generate-lockfile", "--offline", "--manifest-path"])
            .arg(manifest),
    )
}

pub fn app_build(root: &Path, args: &[String]) -> Result<()> {
    let [path] = args else {
        return Err("Usage: cargo xtask app-build PROJECT".into());
    };
    let project = project_path(root, path)?;
    safe_directory(&project)?;
    checked_file(&project.join("project.toml"))?;
    let metadata: Project = toml::from_str(&fs::read_to_string(project.join("project.toml"))?)?;
    if metadata.schema != 1
        || metadata.version == 0
        || !matches!(metadata.language.as_str(), "c" | "cpp" | "rust")
    {
        return Err("Unsupported application schema, language or version".into());
    }
    validate_name(&metadata.name)?;
    let sdk = project.join(".anova-sdk");
    checked_file(&sdk.join("build.py"))?;
    command(
        Command::new("python3")
            .arg(sdk.join("build.py"))
            .arg(&metadata.language)
            .arg(&project),
    )?;
    let binary = project.join("app.elf");
    checked_file(&binary)?;
    let bytes = fs::read(&binary)?;
    anova_elf::parse(&bytes).map_err(|error| format!("Invalid native SDK ELF: {error:?}"))?;
    let mut contract = [0u8; 64];
    contract[..8].copy_from_slice(b"ANOVAABI");
    contract[8..23].copy_from_slice(anova_abi::PERSONALITY.as_bytes());
    contract[40..46].copy_from_slice(b"x86_64");
    contract[56..60].copy_from_slice(&anova_abi::SDK_MAJOR.to_le_bytes());
    if !bytes.ends_with(&contract) {
        return Err("Native SDK build omitted its ABI contract".into());
    }
    write_file(
        &project.join("app.toml"),
        &format!(
            "# SPDX-License-Identifier: Apache-2.0\n# Checked package input; rebuild after changing source or version.\nschema = 1\nname = {:?}\nversion = {}\nabi = \"anova.native.v1\"\nsdk = 1\narchitecture = \"x86_64\"\nentry = \"app.elf\"\nsha256 = \"{:x}\"\npermissions = [\"read\", \"data\"]\ncapabilities = [\"vfs\", \"persist\"]\ndependencies = [{{ name = \"anova.sdk\", min_version = 1 }}]\n",
            metadata.name,
            metadata.version,
            Sha256::digest(&bytes)
        ),
    )?;
    println!(
        "Verified app manifest: {}",
        project.join("app.toml").display()
    );
    Ok(())
}

pub fn verify(root: &Path) -> Result<()> {
    let sdk = build(root)?;
    command(
        Command::new("sha256sum")
            .args(["--check", "SHA256SUMS"])
            .current_dir(&sdk),
    )?;
    for language in ["c", "cpp", "rust"] {
        let path = format!("build/sdk/templates/{language}");
        if !root.join(&path).exists() {
            app_new(root, &[language.into(), path.clone()])?;
        }
        // These paths belong to the acceptance fixtures. Refresh source and SDK snapshots
        // so a repeated gate validates this checkout rather than the preceding SDK release.
        copy_tree(&sdk, &root.join(&path).join(".anova-sdk"))?;
        copy_tree(&sdk.join("templates").join(language), &root.join(&path))?;
        if language == "rust" {
            configure_rust_project(&root.join(&path))?;
        }
        app_build(root, &[path])?;
    }
    let fixtures = root.join("build/sdk/templates");
    for (compiler, language) in [("gcc", "c"), ("g++", "c++")] {
        let output = fixtures.join(format!("contract-{compiler}"));
        output_path(&output)?;
        command(
            Command::new(compiler)
                .args(["-Wall", "-Wextra", "-Werror", "-x", language])
                .arg("-I")
                .arg(sdk.join("include"))
                .arg(root.join("sdk/tests/contract.c"))
                .arg("-o")
                .arg(&output),
        )?;
        command(&mut Command::new(output))?;
    }
    // Run the exported build driver directly as an independent SDK consumer would.
    command(
        Command::new("python3")
            .arg(sdk.join("build.py"))
            .arg("c")
            .arg(fixtures.join("c")),
    )?;
    let consumer = Temporary::new(root, "sdk-standalone-consumer")?;
    copy_tree(&sdk, &consumer.0.join("sdk"))?;
    command(
        Command::new("python3")
            .arg(consumer.0.join("sdk/build.py"))
            .arg("rust")
            .arg(consumer.0.join("sdk/templates/rust")),
    )?;
    let project = fixtures.join("rust");
    let dependency = project.join("offline-message/src/lib.rs");
    let original = fs::read_to_string(&dependency)?;
    let previous = fs::read(project.join("app.elf"))?;
    write_file(
        &dependency,
        &original.replace("offline dependency", "offline dependency rebuilt"),
    )?;
    let rebuild = app_build(root, &[project.display().to_string()]);
    write_file(&dependency, &original)?;
    rebuild?;
    if fs::read(project.join("app.elf"))? == previous {
        return Err("Changing an offline dependency did not rebuild the SDK application".into());
    }
    app_build(root, &[project.display().to_string()])?;
    let source = fixtures.join("c/main.c");
    let original = fs::read_to_string(&source)?;
    let previous = fs::read(fixtures.join("c/app.elf"))?;
    write_file(
        &source,
        "/* Invalid source tests failed-build retention. */\n#error M13 expected rejection\n",
    )?;
    let rejected = Command::new("python3")
        .arg(sdk.join("build.py"))
        .arg("c")
        .arg(fixtures.join("c"))
        .output()?;
    write_file(&source, &original)?;
    if rejected.status.success() || fs::read(fixtures.join("c/app.elf"))? != previous {
        return Err("A failed SDK build replaced the preceding runnable output".into());
    }
    // Revalidate and regenerate the manifest after the independent build invocation.
    app_build(root, &[fixtures.join("c").display().to_string()])?;
    println!("[M13:SDK] C, C++ and locked offline Rust templates built and native ELF validated");
    Ok(())
}

fn validate_name(name: &str) -> Result<()> {
    if name.is_empty()
        || name.len() > 23
        || !name.as_bytes()[0].is_ascii_lowercase()
        || !name
            .bytes()
            .all(|value| value.is_ascii_lowercase() || value.is_ascii_digit() || value == b'-')
    {
        return Err("Application names must be 1..23 lowercase letters, digits or hyphens, starting with a letter".into());
    }
    Ok(())
}

fn project_path(root: &Path, value: &str) -> Result<PathBuf> {
    let path = Path::new(value);
    if path
        .components()
        .any(|component| matches!(component, std::path::Component::ParentDir))
    {
        return Err("Project paths must not contain parent traversal".into());
    }
    Ok(if path.is_absolute() {
        path.to_owned()
    } else {
        root.join(path)
    })
}

pub(super) fn safe_directory(path: &Path) -> Result<()> {
    let mut current = PathBuf::new();
    for component in path.components() {
        current.push(component);
        match fs::symlink_metadata(&current) {
            Ok(metadata) if metadata.file_type().is_dir() => (),
            Ok(_) => {
                return Err(format!(
                    "Expected directory, refusing symlink: {}",
                    current.display()
                )
                .into());
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => fs::create_dir(&current)?,
            Err(error) => return Err(error.into()),
        }
    }
    Ok(())
}

fn write_file(path: &Path, content: &str) -> Result<()> {
    safe_directory(path.parent().ok_or("Output has no parent")?)?;
    output_path(path)?;
    fs::write(path, content)?;
    Ok(())
}

pub(super) fn copy_file(source: &Path, destination: &Path) -> Result<()> {
    checked_file(source)?;
    safe_directory(destination.parent().ok_or("Output has no parent")?)?;
    output_path(destination)?;
    fs::copy(source, destination)?;
    Ok(())
}

pub(super) fn copy_tree(source: &Path, destination: &Path) -> Result<()> {
    safe_directory(destination)?;
    for entry in fs::read_dir(source)? {
        let entry = entry?;
        let path = destination.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_tree(&entry.path(), &path)?;
        } else {
            copy_file(&entry.path(), &path)?;
        }
    }
    Ok(())
}

pub(super) fn collect_files(
    base: &Path,
    directory: &Path,
    output: &mut Vec<PathBuf>,
) -> Result<()> {
    for entry in fs::read_dir(directory)? {
        let entry = entry?;
        if entry.file_type()?.is_dir() {
            collect_files(base, &entry.path(), output)?;
        } else {
            checked_file(&entry.path())?;
            output.push(entry.path().strip_prefix(base)?.to_owned());
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn source_manifest_rejects_unknown_identity_and_path_traversal() {
        assert!(validate_name("native-demo").is_ok());
        for name in ["", "-demo", "../demo", "Native", "a/b", "a b"] {
            assert!(validate_name(name).is_err());
        }
        assert!(project_path(Path::new("/tmp"), "../escape").is_err());
        assert!(
            toml::from_str::<Project>(
                "schema=1\nname='demo'\nlanguage='c'\nversion=1\nabi='linux.x86_64.v1'"
            )
            .is_err()
        );
    }

    #[test]
    fn sdk_export_rejects_stale_extras_without_touching_previous_release() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
        let work = Temporary::new(root, "sdk-export-unit").unwrap();
        let candidate = work.0.join("candidate");
        let previous = work.0.join("release");
        let backup = work.0.join("backup");
        write_file(&candidate.join("include/anova.h"), "current SDK").unwrap();
        write_file(&previous.join("include/anova.h"), "preceding SDK").unwrap();
        write_file(
            &previous.join("rust/target/user-artifact"),
            "retained user data",
        )
        .unwrap();
        let error = publish_export(&candidate, &previous, &backup)
            .unwrap_err()
            .to_string();
        assert!(error.contains("Unexpected SDK export content"));
        assert_eq!(
            fs::read_to_string(previous.join("include/anova.h")).unwrap(),
            "preceding SDK"
        );
        assert_eq!(
            fs::read_to_string(previous.join("rust/target/user-artifact")).unwrap(),
            "retained user data"
        );
        assert_eq!(
            fs::read_to_string(candidate.join("include/anova.h")).unwrap(),
            "current SDK"
        );
        assert!(!backup.exists());
    }

    #[test]
    fn sdk_export_publishes_only_a_complete_known_file_set() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
        let work = Temporary::new(root, "sdk-export-unit").unwrap();
        let candidate = work.0.join("candidate");
        let previous = work.0.join("release");
        let backup = work.0.join("backup");
        write_file(&candidate.join("include/anova.h"), "current SDK").unwrap();
        write_file(&candidate.join("SHA256SUMS"), "current inventory").unwrap();
        write_file(&previous.join("include/anova.h"), "preceding SDK").unwrap();
        publish_export(&candidate, &previous, &backup).unwrap();
        assert_eq!(
            fs::read_to_string(previous.join("include/anova.h")).unwrap(),
            "current SDK"
        );
        assert_eq!(
            fs::read_to_string(previous.join("SHA256SUMS")).unwrap(),
            "current inventory"
        );
        assert_eq!(
            fs::read_to_string(backup.join("include/anova.h")).unwrap(),
            "preceding SDK"
        );
        assert!(!candidate.exists());
    }

    #[cfg(unix)]
    #[test]
    fn sdk_export_rejects_links_instead_of_replacing_their_targets() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
        let work = Temporary::new(root, "sdk-export-unit").unwrap();
        let candidate = work.0.join("candidate");
        let previous = work.0.join("release");
        let outside = work.0.join("outside");
        write_file(&candidate.join("anova.h"), "current SDK").unwrap();
        write_file(&outside, "unrelated data").unwrap();
        safe_directory(&previous).unwrap();
        std::os::unix::fs::symlink(&outside, previous.join("anova.h")).unwrap();
        assert!(publish_export(&candidate, &previous, &work.0.join("backup")).is_err());
        assert!(previous.join("anova.h").is_symlink());
        assert_eq!(fs::read_to_string(outside).unwrap(), "unrelated data");
    }
}
