//! A repo's working copy, conflict state and summary (UI-3, UI-15, OPS-1,
//! PERF-3), read on a blocking thread so the async runtime's workers never
//! wait on the disk or on git (ARCH-7).

use gitcroc_git::{RepoState, RepoSummary, StatusError, StatusRead, WorkingCopy};

/// Why a status, state or summary read failed.
#[derive(Debug, thiserror::Error)]
pub enum StatusReadError {
    #[error(transparent)]
    Read(#[from] StatusError),
    #[error("the status read did not finish: {0}")]
    Stopped(#[from] tokio::task::JoinError),
}

/// The working copy; see [`StatusRead::status`].
pub async fn working_copy<R>(reader: R) -> Result<WorkingCopy, StatusReadError>
where
    R: StatusRead + Send + 'static,
{
    Ok(tokio::task::spawn_blocking(move || reader.status()).await??)
}

/// The conflict state; see [`StatusRead::repo_state`].
pub async fn repo_state<R>(reader: R) -> Result<RepoState, StatusReadError>
where
    R: StatusRead + Send + 'static,
{
    Ok(tokio::task::spawn_blocking(move || reader.repo_state()).await??)
}

/// The summary; see [`StatusRead::summary`].
pub async fn repo_summary<R>(reader: R) -> Result<RepoSummary, StatusReadError>
where
    R: StatusRead + Send + 'static,
{
    Ok(tokio::task::spawn_blocking(move || reader.summary()).await??)
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use gitcroc_git::{GitRepo, GitRunner, Head, Operation, StatusReader};
    use repo_gen::Fixture;

    use super::*;

    /// A stand-in for another implementation behind the same trait.
    struct Merging;

    impl StatusRead for Merging {
        fn status(&self) -> Result<WorkingCopy, StatusError> {
            Ok(WorkingCopy::default())
        }

        fn repo_state(&self) -> Result<RepoState, StatusError> {
            Ok(RepoState {
                operation: Some(Operation::Merge),
                unmerged_paths: 0,
            })
        }

        fn summary(&self) -> Result<RepoSummary, StatusError> {
            Err(StatusError::NoWorkTree)
        }
    }

    #[tokio::test]
    async fn arch_2_status_reads_go_through_the_read_trait() {
        assert!(working_copy(Merging).await.unwrap().is_clean());
        let state = repo_state(Merging).await.unwrap();
        assert_eq!(state.operation, Some(Operation::Merge));
        assert!(matches!(
            repo_summary(Merging).await,
            Err(StatusReadError::Read(StatusError::NoWorkTree))
        ));
    }

    #[tokio::test]
    async fn arch_7_core_reads_status_off_the_async_workers() {
        let fixture = Fixture::new().unwrap();
        let git = fixture.repo("app").unwrap();
        let hash = git.commit_file("a.txt", "a\n", "First").unwrap();
        git.write("a.txt", "b\n").unwrap();
        let reader = StatusReader::new(
            GitRepo::open(git.path()).unwrap(),
            GitRunner::new(PathBuf::from("git")),
        );

        let files = working_copy(reader.clone()).await.unwrap();
        assert_eq!(files.unstaged.len(), 1);
        assert!(!repo_state(reader.clone()).await.unwrap().in_conflict());
        let summary = repo_summary(reader).await.unwrap();
        assert_eq!(
            summary.head,
            Head::Branch {
                name: "main".into(),
                hash: Some(hash)
            }
        );
        assert!(summary.dirty);
    }
}
