use clap::Parser;
use git2::{Commit, ObjectType, Repository, Revwalk, Sort};
use lazy_static::lazy_static;
use regex::{Captures, Regex};
use std::cmp::Ordering;
use std::fmt::{self, Display};
use std::path::{Path, PathBuf};
use std::str::FromStr;
use thiserror::Error;

/// Defines the type of version bump based on commit message analysis.
#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Clone, Copy)]
enum BumpType {
    Major,
    Minor,
    Patch,
    None,
}

/// Represents a semantic version number.
#[derive(Debug, Default, PartialEq, Eq, Clone)]
struct Version {
    major: u64,
    minor: u64,
    patch: u64,
}

/// Represents a Git tag that follows semantic versioning.
#[derive(Debug, Eq, PartialEq, Clone)]
struct SemVerTag {
    name: String,
    version: Version,
}

/// Command-line arguments for the application.
#[derive(Parser, Debug)]
#[command(version = "0.1.0", about = "Automated semantic versioning using git tags.", long_about = None)]
struct Cli {
    /// Only show important output
    #[arg(short, long, default_value_t = false)]
    quiet: bool,

    /// Check whether this repository has tags according ot semantic versioning
    #[arg(long, default_value_t = false)]
    check: bool,

    /// Do not make any changes to the repository
    #[arg(long, default_value_t = false)]
    dry_run: bool,

    /// Tags will only be created locally and not pushed to the remote
    #[arg(long, default_value_t = false)]
    no_push: bool,

    /// Creates the initial tag by analyzing all commits
    #[arg(long, default_value_t = false)]
    auto_init: bool,

    /// The version to start from if --auto-init-tag is used
    #[arg(long, default_value = "0.1.0")]
    initial_version: String,

    /// Path to the git repository
    #[arg(long, default_value = ".")]
    repository_path: PathBuf,

    /// Creates or moves the major version tag (e.g., v5)
    #[arg(long)]
    with_major: bool,

    /// Creates or moves the minor version tag (e.g., v5.2)
    #[arg(long)]
    with_minor: bool,

    /// Creates or moves the patch version tag (e.g., v5.2.1)
    #[arg(long)]
    with_patch: bool,

    /// Whether to move existing tags
    #[arg(long)]
    move_versions: bool,
}

/// Application-specific errors.
#[derive(Error, Debug)]
enum CorelError {
    #[error("Git error: {0}")]
    Git(#[from] git2::Error),
    #[error("Regex error: {0}")]
    Regex(#[from] regex::Error),
    #[error("Failed to parse initial version: '{0}'")]
    InvalidInitialVersion(String),
    #[error("No tags found and --auto-init-tag was not provided")]
    NoTagsNoAutoInit,
    #[error("The repository contains no commits")]
    NoCommits,
    #[error("Could not find the commit for tag '{0}'")]
    TagCommitNotFound(String),
}

type Result<T, E = CorelError> = std::result::Result<T, E>;

lazy_static! {
    static ref SEMVER_REGEX: Regex = Regex::new(
        r"^v?(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)(?:-.*)?(?:\+.*)?$"
    ).unwrap();
    static ref MAJOR_REGEX: Regex = Regex::new(r"^(BREAKING CHANGE)\s?(\(.+\))?\s?:(.+)").unwrap();
    static ref MINOR_REGEX: Regex = Regex::new(r"^(feat|refactor)\s?(\(.+\))?\s?:(.+)").unwrap();
}

impl PartialOrd for SemVerTag {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for SemVerTag {
    fn cmp(&self, other: &Self) -> Ordering {
        self.version.cmp(&other.version)
    }
}

impl Ord for Version {
    fn cmp(&self, other: &Self) -> Ordering {
        self.major
            .cmp(&other.major)
            .then_with(|| self.minor.cmp(&other.minor))
            .then_with(|| self.patch.cmp(&other.patch))
    }
}

impl PartialOrd for Version {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Display for Version {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}.{}.{}", self.major, self.minor, self.patch)
    }
}

impl FromStr for Version {
    type Err = ();

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        let caps: Captures = SEMVER_REGEX.captures(s).ok_or(())?;
        let major = caps.get(1).unwrap().as_str().parse().map_err(|_| ())?;
        let minor = caps.get(2).unwrap().as_str().parse().map_err(|_| ())?;
        let patch = caps.get(3).unwrap().as_str().parse().map_err(|_| ())?;
        Ok(Version { major, minor, patch })
    }
}

impl Version {
    /// Bumps the version according to the specified bump type.
    fn bump(&mut self, bump_type: BumpType) {
        match bump_type {
            BumpType::Major => {
                if self.major > 0 {
                    self.major += 1;
                    self.minor = 0;
                    self.patch = 0;
                } else {
                    self.minor += 1;
                    self.patch = 0;
                }
            }
            BumpType::Minor => {
                self.minor += 1;
                self.patch = 0;
            }
            BumpType::Patch => {
                self.patch += 1;
            }
            BumpType::None => {}
        }
    }
}

/// Analyzes a commit message to determine the required version bump.
fn analyze_commit_message(message: &str) -> BumpType {
    if MAJOR_REGEX.is_match(message) {
        BumpType::Major
    } else if MINOR_REGEX.is_match(message) {
        BumpType::Minor
    } else {
        BumpType::Patch
    }
}

/// Collects commits from a repository since a specified commit.
fn collect_commits<'repo>(repo: &'repo Repository, since_oid: Option<git2::Oid>) -> Result<Vec<Commit<'repo>>> {
    let mut revwalk: Revwalk = repo.revwalk()?;
    revwalk.push_head()?;
    revwalk.set_sorting(Sort::TOPOLOGICAL | Sort::TIME | Sort::REVERSE)?;

    if let Some(oid) = since_oid {
        revwalk.hide(oid)?;
    }

    let commits: std::result::Result<Vec<Commit>, git2::Error> = revwalk
        .filter_map(|oid| oid.ok())
        .map(|oid| repo.find_commit(oid))
        .collect();

    Ok(commits?)
}

/// Finds the latest semantic version tag in the repository.
fn find_latest_semver_tag(repo: &Repository) -> Result<Option<SemVerTag>> {
    let tags = repo.tag_names(None)?;
    let latest_tag = tags
        .iter()
        .filter_map(|tag_name| tag_name)
        .filter_map(|tag_name| {
            Version::from_str(tag_name).ok().map(|version| SemVerTag {
                name: tag_name.to_string(),
                version,
            })
        })
        .max();
    Ok(latest_tag)
}

/// Creates a new lightweight Git tag, overwriting if it exists.
fn create_or_move_tag(repo: &Repository, tag_name: &str, target_rev: &str, args: &Cli) -> Result<()> {
    if args.dry_run {
        if !args.quiet {
            eprintln!("[DRY RUN] Would create tag '{}'", tag_name);
        }
        return Ok(());
    }

    let target_obj = repo.revparse_single(target_rev)?;
    let result = repo.tag_lightweight(tag_name, &target_obj, args.move_versions);
    if result.is_ok() && !args.quiet {
        eprintln!("Successfully created tag '{}'", tag_name);
    }
    Ok(())
}

/// Handles the logic for creating the first tag if none exist.
fn auto_initialize_tag(repo: &Repository, args: &Cli) -> Result<Vec<String>> {
    if !args.auto_init {
        return Err(CorelError::NoTagsNoAutoInit);
    }

    if !args.quiet {
        eprintln!("No tags found. Analyzing all commits to create initial version from {}.", args.initial_version);
    }

    let mut version = Version::from_str(&args.initial_version)
        .map_err(|_| CorelError::InvalidInitialVersion(args.initial_version.clone()))?;

    let commits = collect_commits(repo, None)?;
    if commits.is_empty() {
        return Err(CorelError::NoCommits);
    }

    if !args.quiet {
        eprintln!("Found {} commits to analyze for initial version.", commits.len());
    }

    for commit in commits.iter().skip(1) {
        let bump = analyze_commit_message(commit.message().unwrap_or(""));
        version.bump(bump);
    }

    let mut tags_to_create = Vec::new();
    if args.with_patch {
        tags_to_create.push(version.to_string());
    }
    if args.with_minor {
        tags_to_create.push(format!("{}.{}", version.major, version.minor));
    }
    if args.with_major {
        tags_to_create.push(format!("{}", version.major));
    }

    for tag_name in &tags_to_create {
        create_or_move_tag(repo, tag_name, "HEAD", args)?;
    }

    Ok(tags_to_create)
}

/// Check whether this repository has semantic versioning enabled
fn check(args: Cli) -> Result<bool> {
    let repo_path: &Path = &args.repository_path;
    let repo = Repository::open(repo_path)?;

    if repo.head().is_err() || collect_commits(&repo, None)?.is_empty() {
        return Err(CorelError::NoCommits);
    }

    let has_latest_tag = match find_latest_semver_tag(&repo)? {
        Some(_) => true,
        None => {
            auto_initialize_tag(&repo, &args).is_ok()
        }
    };

    Ok(has_latest_tag)
}

// Returns a list of the latest tags, one requested a version bump for
fn run(args: Cli) -> Result<Vec<String>> {
    let repo_path: &Path = &args.repository_path;
    let repo = Repository::open(repo_path)?;

    if repo.head().is_err() || collect_commits(&repo, None)?.is_empty() {
        return Err(CorelError::NoCommits);
    }

    let latest_tag = match find_latest_semver_tag(&repo)? {
        Some(tag) => tag,
        None => {
            return auto_initialize_tag(&repo, &args);
        }
    };

    if !args.quiet {
        eprintln!("Latest semantic tag found: {}", latest_tag.name);
    }

    let tag_object = repo.revparse_single(&latest_tag.name)?;
    let tag_commit_oid = match tag_object.kind() {
        Some(ObjectType::Commit) => tag_object.as_commit().unwrap().id(),
        Some(ObjectType::Tag) => tag_object.as_tag().unwrap().target()?.id(),
        _ => return Err(CorelError::TagCommitNotFound(latest_tag.name.clone())),
    };

    let highest_bump =  {
        let commits = collect_commits(&repo, Some(tag_commit_oid))?;
        commits
            .iter()
            .map(|c| analyze_commit_message(c.message().unwrap_or("")))
            .min() // Major(0) < Minor(1) < Patch(2), so min gives highest precedence
            .unwrap_or(BumpType::None)
    };

    let mut next_version = latest_tag.version.clone();
    next_version.bump(highest_bump);

    let mut tags_to_create = Vec::new();

    if args.with_patch {
        tags_to_create.push(next_version.to_string());
    }
    if args.with_minor {
        tags_to_create.push(format!("{}.{}", next_version.major, next_version.minor));
    }
    if args.with_major {
        tags_to_create.push(format!("{}", next_version.major));
    }

    if !args.quiet && !tags_to_create.is_empty() {
        eprintln!(
            "Calculated version: {}. Creating tags: {}",
            next_version,
            tags_to_create.join(", ")
        );
    }

    for tag_name in &tags_to_create {
        create_or_move_tag(&repo, tag_name, "HEAD", &args)?;
    }

    Ok(tags_to_create)
}

fn main() {
    use std::process::exit;

    let args = Cli::parse();

    if args.check {
        match check(args) {
            Ok(true) => exit(0),
            Ok(false) => exit(1),
            Err(_) => exit(10),
        }
    }

    match run(args) {
        Ok(tags) => {
            if !tags.is_empty() {
                println!("{}", tags.join(" "));
            }
        }
        Err(e) => {
            eprintln!("ERROR: {}", e);
            exit(1);
        }
    }
}
