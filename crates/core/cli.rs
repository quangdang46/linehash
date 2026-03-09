use std::path::PathBuf;

use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(
    name = "linehash",
    version,
    about = "Hash-anchored file editing for agents"
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand)]
pub enum Commands {
    Read(ReadCmd),
    Index(IndexCmd),
    Edit(EditCmd),
    Insert(InsertCmd),
    Delete(DeleteCmd),
    Verify(VerifyCmd),
    Grep(GrepCmd),
    Annotate(AnnotateCmd),
    Patch(PatchCmd),
    Swap(SwapCmd),
    Move(MoveCmd),
    Indent(IndentCmd),
    FindBlock(FindBlockCmd),
    Stats(StatsCmd),
    FromDiff(FromDiffCmd),
    MergePatches(MergePatchesCmd),
    Watch(WatchCmd),
    Explode(ExplodeCmd),
    Implode(ImplodeCmd),
}

#[derive(Parser)]
pub struct ReadCmd {
    pub file: PathBuf,
    #[arg(long)]
    pub anchor: Vec<String>,
    #[arg(long, default_value = "5")]
    pub context: usize,
    #[arg(long)]
    pub json: bool,
}

#[derive(Parser)]
pub struct IndexCmd {
    pub file: PathBuf,
    #[arg(long)]
    pub json: bool,
}

#[derive(Parser)]
pub struct EditCmd {
    pub file: PathBuf,
    pub anchor: String,
    pub content: String,
    #[arg(long)]
    pub dry_run: bool,
    #[arg(long)]
    pub receipt: bool,
    #[arg(long)]
    pub audit_log: Option<PathBuf>,
    #[arg(long)]
    pub expect_mtime: Option<i64>,
    #[arg(long)]
    pub expect_inode: Option<u64>,
    #[arg(long)]
    pub json: bool,
}

#[derive(Parser)]
pub struct InsertCmd {
    pub file: PathBuf,
    pub anchor: String,
    pub content: String,
    #[arg(long)]
    pub before: bool,
    #[arg(long)]
    pub dry_run: bool,
    #[arg(long)]
    pub receipt: bool,
    #[arg(long)]
    pub audit_log: Option<PathBuf>,
    #[arg(long)]
    pub expect_mtime: Option<i64>,
    #[arg(long)]
    pub expect_inode: Option<u64>,
    #[arg(long)]
    pub json: bool,
}

#[derive(Parser)]
pub struct DeleteCmd {
    pub file: PathBuf,
    pub anchor: String,
    #[arg(long)]
    pub dry_run: bool,
    #[arg(long)]
    pub receipt: bool,
    #[arg(long)]
    pub audit_log: Option<PathBuf>,
    #[arg(long)]
    pub expect_mtime: Option<i64>,
    #[arg(long)]
    pub expect_inode: Option<u64>,
    #[arg(long)]
    pub json: bool,
}

#[derive(Parser)]
pub struct VerifyCmd {
    pub file: PathBuf,
    pub anchors: Vec<String>,
    #[arg(long)]
    pub json: bool,
}

#[derive(Parser)]
pub struct GrepCmd {
    pub file: PathBuf,
    pub pattern: String,
    #[arg(long)]
    pub json: bool,
    #[arg(long)]
    pub invert: bool,
    #[arg(long)]
    pub case_insensitive: bool,
}

#[derive(Parser)]
pub struct AnnotateCmd {
    pub file: PathBuf,
    pub query: String,
    #[arg(long)]
    pub regex: bool,
    #[arg(long)]
    pub expect_one: bool,
    #[arg(long)]
    pub json: bool,
}

#[derive(Parser)]
pub struct PatchCmd {
    pub file: PathBuf,
    pub patch: String,
    #[arg(long)]
    pub dry_run: bool,
    #[arg(long)]
    pub receipt: bool,
    #[arg(long)]
    pub audit_log: Option<PathBuf>,
    #[arg(long)]
    pub expect_mtime: Option<i64>,
    #[arg(long)]
    pub expect_inode: Option<u64>,
    #[arg(long)]
    pub json: bool,
}

#[derive(Parser)]
pub struct SwapCmd {
    pub file: PathBuf,
    pub anchor_a: String,
    pub anchor_b: String,
    #[arg(long)]
    pub dry_run: bool,
    #[arg(long)]
    pub receipt: bool,
    #[arg(long)]
    pub audit_log: Option<PathBuf>,
    #[arg(long)]
    pub expect_mtime: Option<i64>,
    #[arg(long)]
    pub expect_inode: Option<u64>,
}

#[derive(Parser)]
pub struct MoveCmd {
    pub file: PathBuf,
    pub anchor: String,
    pub direction: MoveDirection,
    pub target: String,
    #[arg(long)]
    pub dry_run: bool,
    #[arg(long)]
    pub receipt: bool,
    #[arg(long)]
    pub audit_log: Option<PathBuf>,
    #[arg(long)]
    pub expect_mtime: Option<i64>,
    #[arg(long)]
    pub expect_inode: Option<u64>,
}

#[derive(clap::ValueEnum, Clone, Copy)]
pub enum MoveDirection {
    After,
    Before,
}

#[derive(Parser)]
pub struct IndentCmd {
    pub file: PathBuf,
    pub range: String,
    pub amount: String,
    #[arg(long)]
    pub dry_run: bool,
    #[arg(long)]
    pub receipt: bool,
    #[arg(long)]
    pub audit_log: Option<PathBuf>,
    #[arg(long)]
    pub expect_mtime: Option<i64>,
    #[arg(long)]
    pub expect_inode: Option<u64>,
}

#[derive(Parser)]
pub struct FindBlockCmd {
    pub file: PathBuf,
    pub anchor: String,
    #[arg(long)]
    pub json: bool,
}

#[derive(Parser)]
pub struct StatsCmd {
    pub file: PathBuf,
    #[arg(long)]
    pub json: bool,
}

#[derive(Parser)]
pub struct FromDiffCmd {
    pub file: PathBuf,
    pub diff: String,
    #[arg(long)]
    pub json: bool,
}

#[derive(Parser)]
pub struct MergePatchesCmd {
    pub patch_a: PathBuf,
    pub patch_b: PathBuf,
    #[arg(long)]
    pub base: PathBuf,
    #[arg(long)]
    pub json: bool,
}

#[derive(Parser)]
pub struct WatchCmd {
    pub file: PathBuf,
    #[arg(long)]
    pub once: bool,
    #[arg(long)]
    pub continuous: bool,
    #[arg(long)]
    pub json: bool,
}

#[derive(Parser)]
pub struct ExplodeCmd {
    pub file: PathBuf,
    #[arg(long)]
    pub out: PathBuf,
    #[arg(long)]
    pub force: bool,
}

#[derive(Parser)]
pub struct ImplodeCmd {
    pub dir: PathBuf,
    #[arg(long)]
    pub out: PathBuf,
    #[arg(long)]
    pub dry_run: bool,
}
