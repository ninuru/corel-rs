# corel

**Turn Git commit history into semantic version tags.**

[![Build and tag](https://github.com/ninuru/corel-rs/actions/workflows/build-and-tag.yml/badge.svg)](https://github.com/ninuru/corel-rs/actions/workflows/build-and-tag.yml)

`corel` is a Rust CLI that reads commit messages since the latest semantic version tag, calculates the next version, and creates lightweight Git tags. It can also initialize a repository from its existing history. The [GitHub Actions workflow](#this-project-tags-itself) builds `corel` and uses that binary to tag this repository.

## Get started

Install [Rust and Cargo](https://www.rust-lang.org/tools/install), then build the CLI:

```bash
cargo build --release --locked
```

Run it from a Git repository with at least one commit. Start with a dry run to see the tag it would create:

```bash
./target/release/corel --auto-init --with-patch --dry-run
```

Remove `--dry-run` to create the tag locally:

```bash
tag="$(./target/release/corel --auto-init --with-patch)"
git push origin "refs/tags/$tag"
```

> [!NOTE]
> `corel` creates tags locally; it does not push them. The GitHub workflow pushes the tag explicitly. For a first run, `--auto-init` starts at `0.1.0` by default and analyzes commits after the repository's first commit. Check the dry-run result before creating a tag from a long history.

## How versions are chosen

`corel` picks the highest required bump among commits after the latest numeric semantic version tag. Tags it creates have no `v` prefix, for example `0.2.1`.

| Commit message | Bump |
| --- | --- |
| `feat: add option` or `refactor: simplify parser` | Minor |
| `feat!: change CLI` or a `BREAKING CHANGE:` marker | Major |
| `fix: handle error`, other conventional types, or plain messages | Patch |

While the major version is `0`, a breaking change increments the **minor** version and resets the patch version. With no commits after the latest tag, the version stays the same.

> [!TIP]
> Keep commit messages in [Conventional Commits](https://www.conventionalcommits.org/) format so version changes reflect your intent. Unrecognized messages produce a patch bump.

### Common options

```text
--with-patch           Create the full version tag (for example, 1.2.3)
--with-minor           Also create a minor tag (for example, 1.2)
--with-major           Also create a major tag (for example, 1)
--move-versions        Move existing tags when using --with-minor or --with-major
--auto-init            Calculate the first tag when no version tag exists
--initial-version X.Y.Z  Starting version for --auto-init (default: 0.1.0)
--repository-path PATH Run against another Git repository
--dry-run              Show the tags without creating them
--check                Exit 0 if a version tag exists, 1 if none exists
--quiet                Suppress progress messages
```

Run `corel --help` for the complete option list. The command prints tag names to standard output and progress messages to standard error, so scripts can capture the result.

## This project tags itself

On every pull request and push to `master`, [GitHub Actions](.github/workflows/build-and-tag.yml) runs the Rust tests and builds a release binary. The binary is available as a workflow artifact. After a successful build on `master`, the workflow runs `corel --auto-init --with-patch` against the full Git history and pushes its calculated tag.

The tag job has `contents: write` permission and runs only for pushes to `master`. Pull requests build and test without creating tags. The workflow skips an older run if a newer commit has reached `master`.

To run the same checks locally:

```bash
cargo test --locked
cargo build --release --locked
```
