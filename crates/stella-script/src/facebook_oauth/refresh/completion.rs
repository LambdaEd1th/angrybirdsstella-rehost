//! 2DDE60 starts an independent REST connection, then releases its dependent task.
use super::*;

pub(super) fn start(
    state: &Weak<Mutex<Session>>,
    owner: &SocialPlatformRequestOwner,
    cache: &FacebookTokenCache,
    rest_root: Option<String>,
    app_id: String,
) {
    let Some(shared) = state.upgrade() else {
        return;
    };
    let (token, dispatcher, adapter) = {
        let mut session = shared.lock().expect("Facebook session lock poisoned");
        if !owns(&session, owner) {
            return;
        }
        let now = session.refresh.now();
        match session.refresh.admit_extension(now) {
            Ok(false) => return,
            Err(error) => {
                session.refresh.last_error = Some(error);
                return;
            }
            Ok(true) => (),
        }
        if rest_root.is_none() {
            session.refresh.last_error = Some(SocialPlatformError::Unavailable);
            return;
        }
        (
            session
                .refresh
                .token
                .as_ref()
                .expect("open session token")
                .token
                .clone(),
            session.refresh.dispatcher.clone(),
            session.refresh.system_account.clone(),
        )
    };
    let root = rest_root.expect("validated REST endpoint");
    let task_state = state.clone();
    let task_owner = owner.clone();
    let task_cache = cache.clone();
    let launched = std::thread::Builder::new()
        .name("stella-facebook-extend".into())
        .spawn(move || {
            let Some(shared) = task_state.upgrade() else {
                return;
            };
            if !owns(
                &shared.lock().expect("Facebook session lock poisoned"),
                &task_owner,
            ) {
                return;
            }
            let response = wire::send(
                &format!(
                    "{}/method/auth.extendSSOAccessToken?format=json&sdk=ios&access_token={}",
                    root.trim_end_matches('/'),
                    encode_query(&token)
                ),
                None,
            );
            let (result, close) = match response {
                Ok(response) => {
                    let repair = system_account::Admission {
                        state: task_state.clone(),
                        owner: task_owner.clone(),
                        cache: task_cache.clone(),
                        dispatcher: dispatcher.clone(),
                        adapter,
                        app_id,
                    };
                    let pending =
                        repair.schedule(&response, !(200..300).contains(&response.status));
                    system_account::resolve(response, pending.wait())
                }
                Err(error) => (Err(error), false),
            };
            let owner = task_owner.clone();
            let task = SocialPlatformTask::new(move || {
                apply(&task_state, &owner, &task_cache, close, Some(result), None)
            });
            let _ = queue_task(&shared, &task_owner, &dispatcher, task); // Retirement discards the response.
        });
    if launched.is_err() {
        let mut session = shared.lock().expect("Facebook session lock poisoned");
        if owns(&session, owner) {
            session.refresh.last_error = Some(SocialPlatformError::Transport);
        }
    }
}
