// SPDX-License-Identifier: Apache-2.0
//! Check existing tracked/untracked file-purpose coverage and first-party source headers.
//! Human review still verifies that each explanation describes its actual responsibility.
use super::*;
use std::collections::BTreeSet;

pub fn check(root: &Path) -> Result<()> {
    let inventory = fs::read_to_string(root.join("docs/en/guides/FILE_PURPOSES.md"))?;
    let output = Command::new("git")
        .args([
            "ls-files",
            "--cached",
            "--others",
            "--exclude-standard",
            "-z",
        ])
        .output()?;
    if !output.status.success() {
        return Err("Cannot enumerate project files with git".into());
    }
    let paths = String::from_utf8(output.stdout)?;
    let paths: BTreeSet<_> = paths
        .split('\0')
        .filter(|path| !path.is_empty() && root.join(path).is_file())
        .collect();
    let mut errors = Vec::new();
    for path in &paths {
        if !inventory.contains(&format!("`{path}`")) {
            errors.push(format!("{path}: missing FILE_PURPOSES entry"));
        }
        let file = root.join(path);
        if matches!(
            file.extension().and_then(|value| value.to_str()),
            Some("md" | "lock" | "json")
        ) || matches!(*path, "LICENSE" | "NOTICE")
        {
            continue;
        }
        let text = fs::read_to_string(&file)?;
        let rust = file.extension().is_some_and(|value| value == "rs");
        let purpose = text.lines().take(20).any(|line| {
            let line = line.trim();
            let comment = if rust {
                line.strip_prefix("//!")
            } else {
                line.strip_prefix('#')
                    .or_else(|| line.strip_prefix("//"))
                    .or_else(|| line.strip_prefix("/*"))
            };
            comment.is_some_and(|value| {
                !value.contains("SPDX-License-Identifier") && value.trim().len() >= 16
            })
        });
        if !purpose {
            errors.push(format!("{path}: missing top-of-file purpose comment"));
        }
    }
    if errors.is_empty() {
        println!("File-purpose coverage checked for {} files", paths.len());
        Ok(())
    } else {
        Err(errors.join("\n").into())
    }
}
