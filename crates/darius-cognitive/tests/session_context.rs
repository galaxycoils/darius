use darius_cognitive::TurnContext;

#[test]
fn session_identity_fallback_is_stable_and_explicit_identity_is_opaque() {
    let default = TurnContext::new();
    assert!(!default.session_id().is_empty());
    assert_eq!(default.session_id(), TurnContext::default().session_id());
    assert_eq!(
        default.session_id(),
        TurnContext::with_timeout(std::time::Duration::ZERO).session_id()
    );
    let token = tokio_util::sync::CancellationToken::new();
    let ctx = TurnContext::with_token(token.clone());
    assert_eq!(ctx.session_id(), default.session_id());
    let deadline = ctx.execution_context().deadline;
    let ctx = ctx.with_session_id("opaque/Session:ABC");
    assert_eq!(ctx.execution_context().deadline, deadline);
    assert_eq!(ctx.session_id(), "opaque/Session:ABC");
    assert_eq!(ctx.clone().session_id(), ctx.session_id());
    token.cancel();
    assert!(ctx.is_cancelled());
}
