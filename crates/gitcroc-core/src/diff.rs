//! Commit headers, changed files and one file's diff (UI-14, ARCH-8), read
//! on a blocking thread so the async runtime's workers never wait on the
//! disk (ARCH-7).

use gitcroc_git::{
    ChangedFile, CommitDetails, DiffError, DiffOptions, DiffRead, DiffSource, FileDiff,
};

/// Why a commit or diff read failed.
#[derive(Debug, thiserror::Error)]
pub enum DiffReadError {
    #[error(transparent)]
    Read(#[from] DiffError),
    #[error("the diff read did not finish: {0}")]
    Stopped(#[from] tokio::task::JoinError),
}

/// The header of a commit; see [`DiffRead::commit`].
pub async fn commit_details<R>(repo: R, hash: String) -> Result<CommitDetails, DiffReadError>
where
    R: DiffRead + Send + 'static,
{
    Ok(tokio::task::spawn_blocking(move || repo.commit(&hash)).await??)
}

/// The files a commit changed; see [`DiffRead::changed_files`].
pub async fn changed_files<R>(
    repo: R,
    hash: String,
    parent: usize,
) -> Result<Vec<ChangedFile>, DiffReadError>
where
    R: DiffRead + Send + 'static,
{
    Ok(tokio::task::spawn_blocking(move || repo.changed_files(&hash, parent)).await??)
}

/// The diff of one file; see [`DiffRead::file_diff`].
pub async fn file_diff<R>(
    repo: R,
    source: DiffSource,
    file: ChangedFile,
    options: DiffOptions,
) -> Result<FileDiff, DiffReadError>
where
    R: DiffRead + Send + 'static,
{
    Ok(tokio::task::spawn_blocking(move || repo.file_diff(&source, &file, options)).await??)
}

#[cfg(test)]
mod tests {
    use gitcroc_git::GitRepo;
    use repo_gen::Fixture;

    use super::*;

    /// A stand-in for a git CLI fallback behind the same trait.
    struct Fallback;

    impl DiffRead for Fallback {
        fn commit(&self, hash: &str) -> Result<CommitDetails, DiffError> {
            Err(DiffError::NoParent {
                commit: hash.into(),
                index: 0,
            })
        }

        fn changed_files(
            &self,
            _hash: &str,
            _parent: usize,
        ) -> Result<Vec<ChangedFile>, DiffError> {
            Ok(Vec::new())
        }

        fn file_diff(
            &self,
            _source: &DiffSource,
            _file: &ChangedFile,
            _options: DiffOptions,
        ) -> Result<FileDiff, DiffError> {
            Err(DiffError::FileMismatch)
        }
    }

    #[tokio::test]
    async fn arch_2_diff_reads_go_through_the_read_trait() {
        assert!(
            changed_files(Fallback, "c0ffee".into(), 0)
                .await
                .unwrap()
                .is_empty()
        );
        let error = commit_details(Fallback, "c0ffee".into()).await;
        assert!(matches!(
            error,
            Err(DiffReadError::Read(DiffError::NoParent { .. }))
        ));
    }

    #[tokio::test]
    async fn arch_8_core_forwards_one_file_diff() {
        let fixture = Fixture::new().unwrap();
        let git = fixture.repo("app").unwrap();
        git.commit_file("a.txt", "a\n", "First").unwrap();
        let hash = git.commit_file("a.txt", "a\nb\n", "Second").unwrap();
        let repo = GitRepo::open(git.path()).unwrap();

        let details = commit_details(repo.clone(), hash.clone()).await.unwrap();
        assert_eq!(details.message, "Second\n");
        let files = changed_files(repo.clone(), hash.clone(), 0).await.unwrap();
        assert_eq!(files.len(), 1);
        let source = DiffSource::Commit { hash, parent: 0 };
        let diff = file_diff(
            repo.clone(),
            source,
            files[0].clone(),
            DiffOptions::default(),
        )
        .await
        .unwrap();
        assert!(matches!(diff, FileDiff::Text(text) if text.added == 1));

        let missing = commit_details(repo, "nope".into()).await;
        assert!(matches!(
            missing,
            Err(DiffReadError::Read(DiffError::UnknownCommit { .. }))
        ));
    }
}
