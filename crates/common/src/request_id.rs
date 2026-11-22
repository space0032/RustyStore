//! Request correlation across async tasks.
//!
//! The gateway resolves an `x-request-id` per HTTP request. That value is
//! stored in a task-local so handlers and downstream clients can attach it to
//! outgoing gRPC calls without threading it through every function signature.
//!
//! Spawned tasks do **not** inherit task-locals, so capture [`current`] before
//! `tokio::spawn` and re-enter the scope with [`scope`] (or [`scope_opt`])
//! inside the spawned future.

use std::future::Future;

/// Canonical request-id header and gRPC metadata key (lowercase).
pub const HEADER: &str = "x-request-id";

tokio::task_local! {
    static CURRENT_REQUEST_ID: String;
}

/// The request id scoped to the current task, if any.
pub fn current() -> Option<String> {
    CURRENT_REQUEST_ID.try_with(Clone::clone).ok()
}

/// Run `future` with `id` as the current request id.
pub async fn scope<F: Future>(id: impl Into<String>, future: F) -> F::Output {
    CURRENT_REQUEST_ID.scope(id.into(), future).await
}

/// Run `future` with `id` scoped when present, otherwise run it unchanged.
///
/// Useful in spawned tasks: `scope_opt(captured_id, async move { ... }).await`.
pub async fn scope_opt<F: Future>(id: Option<String>, future: F) -> F::Output {
    match id {
        Some(id) => scope(id, future).await,
        None => future.await,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn current_is_none_outside_scope() {
        assert_eq!(current(), None);
    }

    #[tokio::test]
    async fn scope_exposes_id() {
        let seen = scope("abc", async { current() }).await;
        assert_eq!(seen.as_deref(), Some("abc"));
    }

    #[tokio::test]
    async fn nested_scope_overrides_then_restores() {
        let (inner, outer) = scope("outer", async {
            let inner = scope("inner", async { current() }).await;
            (inner, current())
        })
        .await;
        assert_eq!(inner.as_deref(), Some("inner"));
        assert_eq!(outer.as_deref(), Some("outer"));
    }

    #[tokio::test]
    async fn scope_opt_none_leaves_unset() {
        assert_eq!(scope_opt(None, async { current() }).await, None);
    }

    #[tokio::test]
    async fn scope_opt_some_sets_id() {
        let seen = scope_opt(Some("id".to_string()), async { current() }).await;
        assert_eq!(seen.as_deref(), Some("id"));
    }
}
