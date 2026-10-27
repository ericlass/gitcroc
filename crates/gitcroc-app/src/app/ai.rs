//! AI: the providers' settings, keys, models and self-tests, and the
//! proposal jobs of the conflict editor. AI only proposes (D9).

use std::sync::Arc;

use gitcroc_core::{
    AiEligibilityRead, AiOptions, AiProposalGet, AiResolveAllRequest, AiResolveAllStart,
    AiResolveRequest, AiResolveStart, ProviderDraft, ProviderId, WorkspaceId,
};

use super::{App, AppError};
use crate::ai::{AiError, AiProviders, AiRequestId, AiView};

impl App {
    /// The AI settings and providers, each with whether it has a key, never
    /// the key (SEC-1). Works while AI is off, so the entries stay editable.
    pub async fn ai_providers_get(&self) -> Result<AiView, AppError> {
        // The keychain may be asked; keep it off the caller's thread
        // (ARCH-7).
        let ai = self.inner.ai.clone();
        self.blocking(move || ai.view()).await
    }

    /// Saves a provider, new without an id. `key` absent keeps the stored
    /// key, an empty one removes it; a key goes only to the keychain
    /// (SEC-1, D11).
    pub async fn ai_provider_save(
        &self,
        draft: ProviderDraft,
        key: Option<String>,
    ) -> Result<AiView, AppError> {
        self.ai_blocking(move |ai| ai.save(draft, key)).await
    }

    /// Removes a provider and its key (D11); removing the default clears
    /// it.
    pub async fn ai_provider_delete(&self, id: ProviderId) -> Result<AiView, AppError> {
        self.ai_blocking(move |ai| ai.delete(&id)).await
    }

    /// Makes a provider the default (AI-1).
    pub async fn ai_provider_set_default(&self, id: ProviderId) -> Result<AiView, AppError> {
        self.ai_blocking(move |ai| ai.set_default(&id)).await
    }

    /// Changes the AI features switch (AI-16), the exclude list (AI-4) and
    /// the debug log switch (AI-7).
    pub async fn ai_settings_set(&self, options: AiOptions) -> Result<AiView, AppError> {
        self.ai_blocking(move |ai| ai.set_options(options)).await
    }

    /// Keeps the user's consent to sending to `target`, the host or program
    /// the consent dialog showed (AI-6). Refused when the provider sends
    /// elsewhere now, and with `AiOff` while AI is off (AI-16).
    pub async fn ai_provider_consent(
        &self,
        id: ProviderId,
        target: String,
    ) -> Result<AiView, AppError> {
        self.ai_blocking(move |ai| ai.consent(&id, &target)).await
    }

    /// Starts listing a provider's models, saved or not, and returns at
    /// once; the list arrives as `AiModels` (AI-1, ARCH-7). `key` as in
    /// `ai_provider_save`. Refused with `AiOff` while AI is off (AI-16).
    pub async fn ai_models_get(
        &self,
        draft: ProviderDraft,
        key: Option<String>,
    ) -> Result<AiRequestId, AppError> {
        self.ai_blocking(move |ai| ai.models(draft, key)).await
    }

    /// Starts testing a provider, saved or not, and returns at once; the
    /// result arrives as `AiSelfTest` (AI-2, ARCH-7). Refused with `AiOff`
    /// while AI is off (AI-16).
    pub async fn ai_self_test(
        &self,
        draft: ProviderDraft,
        key: Option<String>,
    ) -> Result<AiRequestId, AppError> {
        self.ai_blocking(move |ai| ai.self_test(draft, key)).await
    }

    /// Runs `call` on the providers off the caller's thread: it may wait for
    /// the keychain or write the settings file (ARCH-7).
    async fn ai_blocking<T: Send + 'static>(
        &self,
        call: impl FnOnce(&Arc<AiProviders>) -> Result<T, AiError> + Send + 'static,
    ) -> Result<T, AppError> {
        let ai = self.inner.ai.clone();
        Ok(self.blocking(move || call(&ai)).await??)
    }

    /// Whether unmerged file `file` of view `handle` may go to AI, and why
    /// not (AI-4), so the editor can show the reason. `already_resolved`
    /// when it is no longer in conflict. `None` when a newer view of the
    /// workspace replaced it.
    pub async fn ai_eligibility(
        &self,
        workspace_id: WorkspaceId,
        handle: u32,
        file: u32,
    ) -> Result<Option<AiEligibilityRead>, AppError> {
        let changes = self.inner.changes.clone();
        let exclude = self.inner.settings.ai().exclude;
        Ok(gitcroc_core::ai_eligibility_get(changes, workspace_id, handle, file, exclude).await?)
    }

    /// Resolve with AI (AI-3): refused, in this order, with `ai_off`
    /// (AI-16), `ai_disabled` for the workspace (AI-6), `no_provider` (open
    /// the provider settings), `provider_unusable`, `ineligible` (AI-4) or
    /// `consent_needed` (AI-6); otherwise starts the job "Resolve with AI ·
    /// <file name>" and returns at once. Its result arrives as
    /// `AiProposalReady`. The job only reads the repo and asks the provider
    /// (D9). `None` when a newer view of the workspace replaced the
    /// request's.
    pub async fn ai_resolve(
        &self,
        workspace_id: WorkspaceId,
        request: AiResolveRequest,
    ) -> Result<Option<AiResolveStart>, AppError> {
        let git = self.usable_git().await?;
        Ok(gitcroc_core::ai_resolve(
            self.inner.changes.clone(),
            self.inner.ai_jobs.clone(),
            self.inner.ai.clone(),
            git,
            workspace_id,
            request,
        )
        .await?)
    }

    /// Resolve all with AI (AI-3, AI-13) for the unmerged files of
    /// `request.repo` that view `request.handle` lists: refused, in this
    /// order, with `ai_off` (AI-16), `ai_disabled` for the workspace
    /// (AI-6), `no_provider` (open the provider settings),
    /// `provider_unusable` or, when a file is to be sent, `consent_needed`
    /// (AI-6). Otherwise starts the job "Resolve all with AI · <repo> (n
    /// files)" and returns at once. The job's view carries each file's
    /// progress; each file's result arrives as `AiProposalReady`.
    /// Ineligible files (AI-4), and files whose buffer has every hunk
    /// edited, are listed and not sent. The job only reads the repo and
    /// asks the provider (D9). `None` when a newer view of the workspace
    /// replaced the request's.
    pub async fn ai_resolve_all(
        &self,
        workspace_id: WorkspaceId,
        request: AiResolveAllRequest,
    ) -> Result<Option<AiResolveAllStart>, AppError> {
        let git = self.usable_git().await?;
        Ok(gitcroc_core::ai_resolve_all(
            self.inner.changes.clone(),
            self.inner.ai_jobs.clone(),
            self.inner.ai.clone(),
            git,
            workspace_id,
            request,
        )
        .await?)
    }

    /// The proposal kept for unmerged file `file` of view `handle`, the
    /// reason AI failed for it (AI-9), or `no_proposal`; one made for a
    /// conflict the file no longer has is dropped. `None` when a newer view
    /// of the workspace replaced it.
    pub async fn ai_proposal_get(
        &self,
        workspace_id: WorkspaceId,
        handle: u32,
        file: u32,
    ) -> Result<Option<AiProposalGet>, AppError> {
        let changes = self.inner.changes.clone();
        let ai = self.inner.ai_jobs.clone();
        Ok(gitcroc_core::ai_proposal_get(changes, ai, workspace_id, handle, file).await?)
    }
}
