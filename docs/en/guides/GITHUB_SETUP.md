<!-- Define development branches, source synchronization and documentation-aware contribution requirements. -->
# GitHub repository and collaboration

**English** | [简体中文](../../zh-CN/guides/GITHUB_SETUP.md)

The repository is [ananwanan/AnovaOS](https://github.com/ananwanan/AnovaOS).
Use `dev` for everyday development and pull requests; `main` receives accepted stable snapshots.
Preserve existing visibility, permissions and history. The repository's configured default branch
and the branch checked out locally are independent; creating a branch changes neither setting automatically.

## Get and synchronize the sources

```sh
git clone --branch dev https://github.com/ananwanan/AnovaOS.git
cd AnovaOS
git status --short --branch
git fetch origin --prune
git switch dev
git pull --ff-only origin dev
```

If SSH is already configured, use `git clone --branch dev git@github.com:ananwanan/AnovaOS.git`.
Inspect and preserve local modifications before switching. Do not force a fast-forward failure.

## Start development

Read [AGENTS](../../../AGENTS.md), [current status](STATUS.md), [roadmap](../ROADMAP.md)
and the relevant milestone design and evidence. M0–M14 have accepted bounded profiles;
M15.1/M15.2, guest kernel reconstruction (M15.5) and independent repeat builds (M15.6)
have local acceptance. M15.3/M15.4 assessed compiler reconstruction prerequisites and blockers;
they did not rebuild GCC, LLVM, rustc or Cargo. Current follow-ups are in STATUS.
A candidate future target name does not establish upstream compiler support.
Use `AnovaOS` consistently in documentation.

## Contribution requirements

- Work on `dev` or a task branch based on it; target pull requests at `dev`.
- Promote accepted changes to `main` after verification. Preserve user changes and remote history;
  do not delete the repository or force push to replace history.
- Review status and diff before delivery. Use English commit messages, PR titles/descriptions,
  source comments, Rustdoc and logs; follow [coding style](CODING_STYLE.md).
- Explain every new or modified file near its top and maintain [FILE_PURPOSES](FILE_PURPOSES.md)
  under [DEV-001](REQUIREMENTS.md). Run `cargo xtask check-files` and
  `cargo fmt --all -- --check` with the root formatting configuration.
- Update both README languages, relevant English/Chinese guides, status, roadmap, ABI and evidence
  alongside behavior changes. Report passed, failed and unexecuted validation separately;
  documentation consistency is part of completion.
- Keep images, toolchain caches, credentials, tokens and private logs out of commits.
- Use implemented commands from [DEVELOPMENT](DEVELOPMENT.md). Local results and remote CI results
  are separate evidence.

Original project work uses [Apache-2.0](../../../LICENSE); third-party notices and terms remain
separate. See [LICENSING](LICENSING.md).
