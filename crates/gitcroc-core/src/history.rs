//! History pages (PERF-2, ARCH-8), read on a blocking thread so the async
//! runtime's workers never wait on the disk (ARCH-7).

use gitcroc_git::{Cursor, Filter, HistoryError, HistoryStart, Page, RepoRead};

/// Why a history page could not be produced.
#[derive(Debug, thiserror::Error)]
pub enum HistoryPageError {
    #[error(transparent)]
    Read(#[from] HistoryError),
    #[error("the history read did not finish: {0}")]
    Stopped(#[from] tokio::task::JoinError),
}

/// A page of `repo`'s history; see [`RepoRead::history`].
pub async fn history_page<R>(
    repo: R,
    start: HistoryStart,
    cursor: Option<Cursor>,
    filter: Option<Filter>,
) -> Result<Page, HistoryPageError>
where
    R: RepoRead + Send + 'static,
{
    let page = tokio::task::spawn_blocking(move || repo.history(start, cursor, filter)).await??;
    Ok(page)
}

#[cfg(test)]
mod tests {
    use gitcroc_git::GitRepo;
    use repo_gen::Fixture;

    use super::*;

    /// A stand-in for a git CLI fallback behind the same trait.
    struct Fallback;

    impl RepoRead for Fallback {
        fn history(
            &self,
            start: HistoryStart,
            _cursor: Option<Cursor>,
            _filter: Option<Filter>,
        ) -> Result<Page, HistoryError> {
            match start {
                HistoryStart::Stash(index) => Err(HistoryError::NoStash { index }),
                _ => Ok(Page {
                    rows: Vec::new(),
                    next: None,
                }),
            }
        }
    }

    #[tokio::test]
    async fn arch_2_history_reads_go_through_the_read_trait() {
        let page = history_page(Fallback, HistoryStart::All, None, None)
            .await
            .unwrap();
        assert!(page.rows.is_empty());
        let error = history_page(Fallback, HistoryStart::Stash(3), None, None).await;
        assert!(matches!(
            error,
            Err(HistoryPageError::Read(HistoryError::NoStash { index: 3 }))
        ));
    }

    #[tokio::test]
    async fn perf_2_core_forwards_history_pages() {
        let fixture = Fixture::new().unwrap();
        let git = fixture.repo("app").unwrap();
        git.commit_file("a.txt", "a\n", "First").unwrap();
        git.commit_file("b.txt", "b\n", "Second").unwrap();
        let repo = GitRepo::open(git.path()).unwrap();

        let page = history_page(repo.clone(), HistoryStart::All, None, None)
            .await
            .unwrap();
        let subjects: Vec<&str> = page.rows.iter().map(|row| row.subject.as_str()).collect();
        assert_eq!(subjects, ["Second", "First"]);
        assert!(page.next.is_none());

        let missing = history_page(repo, HistoryStart::Ref("nope".into()), None, None).await;
        assert!(matches!(
            missing,
            Err(HistoryPageError::Read(HistoryError::UnknownRef { .. }))
        ));
    }
}
