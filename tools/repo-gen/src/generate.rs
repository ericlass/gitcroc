//! Generated repos: a planned history written as a `git fast-import` stream.
//!
//! The same [`RepoSpec`] and seed always give the same commit ids: names,
//! dates, messages and file contents all come from [`Rng`], and nothing reads
//! the clock or the environment.

use std::collections::{HashMap, HashSet};
use std::io::{BufWriter, Read, Write};
use std::path::Path;
use std::process::Stdio;

use serde::{Deserialize, Serialize};

use crate::rng::{Rng, mix};
use crate::{Error, git};

/// The shape of one generated repo.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RepoSpec {
    pub name: String,
    /// Tracked files at the tip of `main`.
    pub files: u32,
    /// Lines in those files, in total.
    pub lines: u64,
    /// Commits reachable from all branches, merge commits included.
    pub commits: u32,
    /// Local branches, `main` included.
    pub branches: u32,
}

impl RepoSpec {
    fn validate(&self) -> Result<(), Error> {
        let invalid = |what: &str| Err(Error::Invalid(format!("{}: {what}", self.name)));
        if self.files == 0 || self.branches == 0 {
            return invalid("needs at least one file and one branch");
        }
        if self.lines < u64::from(self.files) {
            return invalid("needs at least one line per file");
        }
        if u64::from(self.commits) < 4 * u64::from(self.branches) {
            return invalid("needs at least four commits per branch");
        }
        Ok(())
    }
}

/// Dates start here (2020-09-13 12:26 UTC) and advance by up to three hours
/// per commit.
const EPOCH: u64 = 1_600_000_000;
const STEP: u64 = 3 * 3600;

/// One tag on `main` every this many commits on `main`.
const TAG_EVERY: u32 = 250;

/// Commits on `main` add 30% of the files at first, then the rest a few at a
/// time.
const INITIAL_SHARE_PERCENT: u64 = 30;

const AUTHORS: [(&str, &str); 12] = [
    ("Ada Moreau", "ada.moreau@example.com"),
    ("Bram Visser", "bram.visser@example.com"),
    ("Chen Li", "chen.li@example.com"),
    ("Dana Okafor", "dana.okafor@example.com"),
    ("Emil Novak", "emil.novak@example.com"),
    ("Farah Haddad", "farah.haddad@example.com"),
    ("Gwen Hughes", "gwen.hughes@example.com"),
    ("Hiro Tanaka", "hiro.tanaka@example.com"),
    ("Ines Duarte", "ines.duarte@example.com"),
    ("Jonas Berg", "jonas.berg@example.com"),
    ("Kavya Rao", "kavya.rao@example.com"),
    ("Luis Ortega", "luis.ortega@example.com"),
];

const WORDS: [&str; 32] = [
    "account", "buffer", "cache", "config", "cursor", "event", "filter", "handler", "index",
    "layout", "loader", "matrix", "message", "module", "parser", "payload", "queue", "record",
    "render", "request", "result", "schema", "session", "signal", "socket", "state", "stream",
    "table", "token", "update", "value", "worker",
];

const VERBS: [&str; 12] = [
    "Add", "Fix", "Update", "Refactor", "Remove", "Rename", "Improve", "Simplify", "Document",
    "Test", "Speed up", "Clean up",
];

const TOP_DIRS: [&str; 8] = [
    "src", "lib", "app", "packages", "services", "tools", "docs", "test",
];

const EXTENSIONS: [&str; 10] = [
    "rs", "ts", "rs", "ts", "py", "go", "md", "json", "toml", "txt",
];

/// What happens at one point of the planned history.
#[derive(Debug, Clone, Copy)]
enum Event {
    Main,
    Branch(usize),
    Merge(usize),
}

/// Orders the commits of `main` and of the side branches in time. A quarter
/// of the commits go to side branches; every second side branch is merged
/// back into `main`.
fn plan(spec: &RepoSpec, rng: &mut Rng) -> Vec<Event> {
    let commits = u64::from(spec.commits);
    let sides = spec.branches as usize - 1;
    let mut sized = Vec::with_capacity(sides);
    let mut budget = commits / 4;
    for side in 0..sides {
        let merged = side % 2 == 1;
        let still_needed = (sides - side - 1) as u64 * 2;
        let room = budget
            .saturating_sub(still_needed + u64::from(merged))
            .max(1);
        let base = (commits / 4 / sides as u64).max(1);
        let count = rng.between(1, 2 * base - 1).min(room);
        budget = budget.saturating_sub(count + u64::from(merged));
        sized.push((count, merged));
    }
    let side_commits: u64 = sized
        .iter()
        .map(|(count, merged)| count + u64::from(*merged))
        .sum();
    let main_commits = commits - side_commits;

    // Each event gets a point in time; sorting by it interleaves the branches.
    const RANGE: u64 = 1 << 40;
    let mut timed: Vec<(u64, usize, Event)> = Vec::with_capacity(spec.commits as usize);
    timed.push((0, 0, Event::Main));
    for _ in 1..main_commits {
        timed.push((1 + rng.below(RANGE), timed.len(), Event::Main));
    }
    for (side, (count, merged)) in sized.into_iter().enumerate() {
        let start = RANGE / 20 + rng.below(RANGE / 20 * 17);
        let span = 1 + rng.below(RANGE / 10);
        for _ in 0..count {
            timed.push((start + rng.below(span), timed.len(), Event::Branch(side)));
        }
        if merged {
            timed.push((start + span, timed.len(), Event::Merge(side)));
        }
    }
    timed.sort_unstable_by_key(|(time, order, _)| (*time, *order));
    timed.into_iter().map(|(_, _, event)| event).collect()
}

/// A side branch while the stream is written.
struct Side {
    name: String,
    tip: Option<u64>,
    /// Files on `main` when the branch was created; it only changes these.
    fork_files: u32,
    versions: HashMap<u32, u32>,
}

/// Writes the stream of one repo.
struct Writer<'a, W: Write> {
    out: W,
    spec: &'a RepoSpec,
    seed: u64,
    rng: Rng,
    lines: Vec<u32>,
    /// The content version of every file on `main`; 0 is the first.
    main_versions: Vec<u32>,
    /// The last version handed out per file, on any branch.
    last_version: Vec<u32>,
    files_on_main: u32,
    main_tip: Option<u64>,
    main_commits: u32,
    main_planned: u32,
    sides: Vec<Side>,
    marks: u64,
    events: u64,
    content: Vec<u8>,
}

/// Line counts that add up to exactly `total`, varying around the average.
fn line_counts(files: u32, total: u64, rng: &mut Rng) -> Vec<u32> {
    let average = total / u64::from(files);
    let mut lines: Vec<u32> = (0..files)
        .map(|_| u32::try_from(rng.between(1, (2 * average).max(2) - 1)).unwrap_or(u32::MAX))
        .collect();
    let mut sum: u64 = lines.iter().map(|&n| u64::from(n)).sum();
    let count = lines.len();
    let mut index = 0;
    while sum != total {
        let n = &mut lines[index % count];
        if sum < total {
            *n += 1;
            sum += 1;
        } else if *n > 1 {
            *n -= 1;
            sum -= 1;
        }
        index += 1;
    }
    lines
}

fn file_path(file: u32) -> String {
    let leaf = file / 40;
    let (outer, inner) = (leaf / 25, leaf % 25);
    let top = TOP_DIRS[outer as usize % TOP_DIRS.len()];
    let ext = EXTENSIONS[(mix(&[u64::from(file)]) % EXTENSIONS.len() as u64) as usize];
    format!("{top}/m{outer:03}/s{inner:02}/f{file:06}.{ext}")
}

fn branch_name(side: usize, rng: &mut Rng) -> String {
    let word = WORDS[rng.below(WORDS.len() as u64) as usize];
    match side % 4 {
        0 | 1 => format!("feature/{word}-{side}"),
        2 => format!("fix/{word}-{side}"),
        _ => format!("release/{}.{side}", 1 + side / 4),
    }
}

impl<'a, W: Write> Writer<'a, W> {
    fn new(out: W, spec: &'a RepoSpec, seed: u64, events: &[Event]) -> Self {
        let mut rng = Rng::new(seed);
        let lines = line_counts(spec.files, spec.lines, &mut rng);
        let sides = (0..spec.branches as usize - 1)
            .map(|side| Side {
                name: branch_name(side, &mut rng),
                tip: None,
                fork_files: 0,
                versions: HashMap::new(),
            })
            .collect();
        let main_planned = events.iter().filter(|e| matches!(e, Event::Main)).count();
        Self {
            out,
            spec,
            seed,
            rng,
            lines,
            main_versions: vec![0; spec.files as usize],
            last_version: vec![0; spec.files as usize],
            files_on_main: 0,
            main_tip: None,
            main_commits: 0,
            main_planned: u32::try_from(main_planned).unwrap_or(u32::MAX),
            sides,
            marks: 0,
            events: 0,
            content: Vec::new(),
        }
    }

    fn write_all(mut self, events: &[Event]) -> Result<W, Error> {
        for event in events {
            match *event {
                Event::Main => self.main_commit()?,
                Event::Branch(side) => self.side_commit(side)?,
                Event::Merge(side) => self.merge(side)?,
            }
        }
        writeln!(self.out, "done")?;
        self.out.flush()?;
        Ok(self.out)
    }

    /// How many files a commit changes: mostly 1 to 20, a few hundreds.
    fn change_count(&mut self, available: u32) -> u32 {
        let count = if self.rng.chance(20) {
            self.rng.between(100, 400)
        } else {
            self.rng.between(1, 20)
        };
        u32::try_from(count).unwrap_or(u32::MAX).min(available)
    }

    /// Distinct files among the first `available`.
    fn pick_files(&mut self, count: u32, available: u32) -> Vec<u32> {
        let mut picked = HashSet::with_capacity(count as usize);
        let mut files = Vec::with_capacity(count as usize);
        while files.len() < count as usize {
            let file = u32::try_from(self.rng.below(u64::from(available))).unwrap_or(0);
            if picked.insert(file) {
                files.push(file);
            }
        }
        files
    }

    fn next_version(&mut self, file: u32) -> u32 {
        let version = &mut self.last_version[file as usize];
        *version += 1;
        *version
    }

    fn main_commit(&mut self) -> Result<(), Error> {
        let files = self.spec.files;
        let adds = if self.main_commits == 0 {
            if self.main_planned <= 1 {
                files
            } else {
                u32::try_from(u64::from(files) * INITIAL_SHARE_PERCENT / 100)
                    .unwrap_or(files)
                    .max(1)
            }
        } else {
            // Spread the remaining files evenly over the remaining commits.
            let left = files - self.files_on_main;
            let commits_left = self.main_planned - self.main_commits;
            left / commits_left + u32::from(!left.is_multiple_of(commits_left))
        };
        let existing = self.files_on_main;
        let changes = if existing == 0 {
            0
        } else {
            self.change_count(existing)
        };
        let changed = self.pick_files(changes, existing);

        let mut entries = Vec::with_capacity(changed.len() + adds as usize);
        for file in changed {
            let version = self.next_version(file);
            self.main_versions[file as usize] = version;
            entries.push((file, version));
        }
        for file in existing..existing + adds {
            entries.push((file, 0));
        }
        self.files_on_main += adds;

        let message = self.message();
        let from = self.main_tip;
        let mark = self.commit("refs/heads/main", &message, from, None, &entries)?;
        self.main_tip = Some(mark);
        self.main_commits += 1;
        if self.main_commits.is_multiple_of(TAG_EVERY) {
            self.tag(mark)?;
        }
        Ok(())
    }

    fn side_commit(&mut self, side: usize) -> Result<(), Error> {
        if self.sides[side].tip.is_none() {
            self.sides[side].tip = self.main_tip;
            self.sides[side].fork_files = self.files_on_main;
        }
        let available = self.sides[side].fork_files;
        let changes = self.change_count(available);
        let mut entries = Vec::with_capacity(changes as usize);
        for file in self.pick_files(changes, available) {
            let version = self.next_version(file);
            self.sides[side].versions.insert(file, version);
            entries.push((file, version));
        }
        let message = self.message();
        let reference = format!("refs/heads/{}", self.sides[side].name);
        let from = self.sides[side].tip;
        let mark = self.commit(&reference, &message, from, None, &entries)?;
        self.sides[side].tip = Some(mark);
        Ok(())
    }

    /// A merge takes the branch's version of each file it changed, unless
    /// `main` has a newer one, which then already contains it.
    fn merge(&mut self, side: usize) -> Result<(), Error> {
        let mut entries: Vec<(u32, u32)> = self.sides[side]
            .versions
            .iter()
            .filter(|&(&file, &version)| version > self.main_versions[file as usize])
            .map(|(&file, &version)| (file, version))
            .collect();
        entries.sort_unstable();
        for &(file, version) in &entries {
            self.main_versions[file as usize] = version;
        }
        let message = format!("Merge branch '{}'", self.sides[side].name);
        let (from, merge) = (self.main_tip, self.sides[side].tip);
        let mark = self.commit("refs/heads/main", &message, from, merge, &entries)?;
        self.main_tip = Some(mark);
        Ok(())
    }

    fn message(&mut self) -> String {
        let verb = VERBS[self.rng.below(VERBS.len() as u64) as usize];
        let first = WORDS[self.rng.below(WORDS.len() as u64) as usize];
        let second = WORDS[self.rng.below(WORDS.len() as u64) as usize];
        let issue = self.rng.between(100, 9999);
        let mut message = format!("{verb} {first} {second} handling (#{issue})\n");
        if self.rng.chance(400) {
            message.push_str(&format!(
                "\nThe {first} now keeps its {second} until the {} is done.\n",
                WORDS[self.rng.below(WORDS.len() as u64) as usize]
            ));
        }
        message
    }

    /// Writes a commit and returns its mark.
    fn commit(
        &mut self,
        reference: &str,
        message: &str,
        from: Option<u64>,
        merge: Option<u64>,
        entries: &[(u32, u32)],
    ) -> Result<u64, Error> {
        self.marks += 1;
        let mark = self.marks;
        let time = EPOCH + self.events * STEP + self.rng.below(STEP);
        self.events += 1;
        let (name, email) = AUTHORS[self.rng.below(AUTHORS.len() as u64) as usize];
        writeln!(self.out, "commit {reference}")?;
        writeln!(self.out, "mark :{mark}")?;
        writeln!(self.out, "author {name} <{email}> {time} +0000")?;
        writeln!(self.out, "committer {name} <{email}> {time} +0000")?;
        self.data(message.as_bytes())?;
        if let Some(from) = from {
            writeln!(self.out, "from :{from}")?;
        }
        if let Some(merge) = merge {
            writeln!(self.out, "merge :{merge}")?;
        }
        for &(file, version) in entries {
            self.render(file, version);
            writeln!(self.out, "M 100644 inline {}", file_path(file))?;
            let content = std::mem::take(&mut self.content);
            self.data(&content)?;
            self.content = content;
        }
        writeln!(self.out)?;
        Ok(mark)
    }

    fn tag(&mut self, mark: u64) -> Result<(), Error> {
        let name = format!("v1.{}.0", self.main_commits / TAG_EVERY);
        let time = EPOCH + self.events * STEP;
        let (author, email) = AUTHORS[0];
        writeln!(self.out, "tag {name}")?;
        writeln!(self.out, "from :{mark}")?;
        writeln!(self.out, "tagger {author} <{email}> {time} +0000")?;
        self.data(format!("Release {name}\n").as_bytes())
    }

    fn data(&mut self, bytes: &[u8]) -> Result<(), Error> {
        writeln!(self.out, "data {}", bytes.len())?;
        self.out.write_all(bytes)?;
        writeln!(self.out)?;
        Ok(())
    }

    /// The content of a file at a version into `self.content`. Version `k`
    /// rewrites a short run of lines chosen by a hash of the file and `k`, so
    /// diffs between versions are small, like real edits.
    fn render(&mut self, file: u32, version: u32) {
        let lines = self.lines[file as usize];
        let mut line_version = vec![0u32; lines as usize];
        for k in 1..=version {
            let hash = mix(&[self.seed, u64::from(file), u64::from(k)]);
            let start = (hash % u64::from(lines)) as usize;
            let len = 1 + ((hash >> 40) % 8) as usize;
            let end = (start + len).min(lines as usize);
            line_version[start..end].fill(k);
        }
        self.content.clear();
        for (line, k) in line_version.into_iter().enumerate() {
            let hash = mix(&[self.seed, u64::from(file), line as u64, u64::from(k)]);
            let word = |shift: u32| WORDS[((hash >> shift) % WORDS.len() as u64) as usize];
            let indent = "    ".repeat(((hash >> 50) % 3) as usize);
            // Writing to a Vec cannot fail.
            let _ = writeln!(
                self.content,
                "{indent}let {}_{} = {}({}, {});",
                word(0),
                hash % 1000,
                word(8),
                word(16),
                (hash >> 24) % 10_000
            );
        }
    }
}

/// Builds a repo at `dir`, which must not exist or be empty: `git init`, the
/// fast-import stream, a repack, the commit-graph file and a checkout of
/// `main`.
pub fn generate_repo(spec: &RepoSpec, seed: u64, dir: &Path) -> Result<(), Error> {
    spec.validate()?;
    std::fs::create_dir_all(dir)?;
    git::run(git::command(
        dir,
        ["init", "--quiet", "--initial-branch=main"],
    ))?;

    let mut command = git::command(
        dir,
        ["fast-import", "--quiet", "--done", "--active-branches=64"],
    );
    command
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::piped());
    let mut child = command.spawn()?;
    let stderr = child.stderr.take().map(|mut pipe| {
        std::thread::spawn(move || {
            let mut text = String::new();
            let _ = pipe.read_to_string(&mut text);
            text
        })
    });
    let written = match child.stdin.take() {
        Some(stdin) => {
            let mut rng = Rng::new(seed);
            let events = plan(spec, &mut rng);
            let out = BufWriter::with_capacity(1 << 20, stdin);
            Writer::new(out, spec, rng.next_u64(), &events)
                .write_all(&events)
                .map(drop)
        }
        None => Err(Error::Invalid("git fast-import has no stdin".into())),
    };
    let status = child.wait()?;
    let stderr = stderr
        .and_then(|thread| thread.join().ok())
        .unwrap_or_default();
    if !status.success() {
        return Err(Error::Git {
            args: git::describe(&command),
            code: status.code(),
            stderr: stderr.trim().to_owned(),
        });
    }
    written?;

    // fast-import packs hardly delta-compress; real clones do.
    git::run(git::command(dir, ["repack", "-a", "-d", "-q"]))?;
    git::run(git::command(dir, ["commit-graph", "write", "--reachable"]))?;
    git::run(git::command(dir, ["reset", "--hard", "--quiet"]))?;
    Ok(())
}
