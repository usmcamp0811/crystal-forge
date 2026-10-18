mod attempt_log_regressions {
    use super::*;
    use crate::auth::session::{SESSION_COOKIE_NAME, hash_token};
    use crate::handlers::agent_request::CFState;
    use crate::handlers::api::commits as handlers;
    use crate::models::auth_identity::AuthRole;
    use crate::queries::{commits, eval_logs};
    use futures::{SinkExt, StreamExt};
    use tokio::time::{Duration, timeout};
    use tokio_tungstenite::tungstenite::{Message, client::IntoClientRequest};

    type Client = tokio_tungstenite::WebSocketStream<
        tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
    >;

    struct TestServer(tokio::task::JoinHandle<()>);
    impl Drop for TestServer {
        fn drop(&mut self) {
            self.0.abort();
        }
    }

    async fn cookie(pool: &PgPool, user: Uuid, role: AuthRole) -> String {
        crate::queries::auth_identity::sync_user_role(pool, user, role)
            .await
            .unwrap();
        let token = Uuid::new_v4().to_string();
        crate::queries::auth_identity::create_user_session(
            pool,
            user,
            hash_token(&token),
            chrono::Utc::now() + chrono::Duration::hours(1),
            None,
            None,
            "local".into(),
        )
        .await
        .unwrap();
        format!("{SESSION_COOKIE_NAME}={token}")
    }

    async fn connect(address: std::net::SocketAddr, commit: i32, cookie: &str) -> Client {
        let mut request = format!("ws://{address}/api/v1/commits/{commit}/eval/stream")
            .into_client_request()
            .unwrap();
        request
            .headers_mut()
            .insert("Cookie", cookie.parse().unwrap());
        timeout(
            Duration::from_secs(5),
            tokio_tungstenite::connect_async(request),
        )
        .await
        .expect("upgrade timeout")
        .expect("authorized upgrade")
        .0
    }

    // An immediate server Ping follows atomic subscribe+replay. Use the protocol
    // barrier, not sleeps, to prove which events were replay and which are live.
    async fn replay_barrier(client: &mut Client) -> Vec<String> {
        timeout(Duration::from_secs(5), async {
            let mut replay = Vec::new();
            loop {
                match client.next().await {
                    Some(Ok(Message::Text(text))) => replay.push(text.to_string()),
                    Some(Ok(Message::Ping(payload))) => {
                        client.send(Message::Pong(payload)).await.unwrap();
                        return replay;
                    }
                    other => panic!("expected replay/Ping barrier, got {other:?}"),
                }
            }
        })
        .await
        .expect("subscription barrier timeout")
    }

    async fn receive_text(client: &mut Client) -> String {
        timeout(Duration::from_secs(5), async {
            loop {
                match client.next().await {
                    Some(Ok(Message::Text(text))) => return text.to_string(),
                    Some(Ok(Message::Ping(payload))) => {
                        client.send(Message::Pong(payload)).await.unwrap()
                    }
                    Some(Ok(Message::Pong(_))) => {}
                    other => panic!("expected live event, got {other:?}"),
                }
            }
        })
        .await
        .expect("live event timeout")
    }

    async fn closed_without_text(client: &mut Client) {
        use tokio_tungstenite::tungstenite::{Error, error::ProtocolError};
        timeout(Duration::from_secs(5), async {
            loop {
                match client.next().await {
                    None | Some(Ok(Message::Close(_))) => return,
                    Some(Err(Error::ConnectionClosed))
                    | Some(Err(Error::Protocol(ProtocolError::ResetWithoutClosingHandshake))) => {
                        return;
                    }
                    Some(Err(Error::Io(error)))
                        if matches!(
                            error.kind(),
                            std::io::ErrorKind::UnexpectedEof | std::io::ErrorKind::ConnectionReset
                        ) =>
                    {
                        return;
                    }
                    Some(Ok(Message::Ping(_))) | Some(Ok(Message::Pong(_))) => {}
                    other => panic!("retired/revoked socket must not send evidence: {other:?}"),
                }
            }
        })
        .await
        .expect("retired/revoked socket closure timeout");
    }

    async fn http_logs(
        address: std::net::SocketAddr,
        commit: i32,
        cookie: &str,
    ) -> reqwest::Response {
        reqwest::Client::builder()
            .timeout(Duration::from_secs(5))
            .build()
            .unwrap()
            .get(format!(
                "http://{address}/api/v1/commits/{commit}/eval/logs"
            ))
            .header("Cookie", cookie)
            .send()
            .await
            .unwrap()
    }

    #[sqlx::test(migrations = "./migrations")]
    #[ignore = "requires isolated migration-backed PostgreSQL"]
    async fn websocket_retry_replaces_hidden_uuid_without_replaying_old_attempt(pool: PgPool) {
        // A captures hidden H under the SAME configuration name as visible hosts.
        // B replaces H with visible V. Latest membership/name coverage must never
        // authorize A's buffered or persisted diagnostics, even for an admin.
        let mut f = fixture_without_plan(&pool, Some(&["alpha"])).await;
        sqlx::query("UPDATE systems SET system_configuration_name = 'alpha' WHERE id = $1")
            .bind(f.second_host)
            .execute(&pool)
            .await
            .unwrap();
        f.plan.rows[1].configuration_name = "alpha".into();
        capture_plan(&pool, &f).await;
        sqlx::query("UPDATE commits SET evaluation_status = 'in_progress', evaluation_attempt_count = $2 WHERE id = $1")
            .bind(f.commit).bind(f.plan.attempt_number).execute(&pool).await.unwrap();
        sqlx::query("UPDATE evaluation_attempts SET status = 'in_progress' WHERE id = $1")
            .bind(f.plan.evaluation_attempt_id)
            .execute(&pool)
            .await
            .unwrap();
        let state = CFState::new(
            pool.clone(),
            crate::config::ServerConfig::default(),
            std::sync::Arc::new(crate::queue::QueueNotifier::new()),
            crate::server::jobs::BackgroundJobRegistry::new(),
        );
        let a = handlers::bind_eval_producer(&state, f.commit, f.plan.attempt_number)
            .await
            .unwrap();
        let a_owner = a.eval_log_producer.unwrap().owner;
        let sentinel = format!("A-private-sentinel-{}", Uuid::new_v4());
        handlers::broadcast_eval_log(&a, f.commit, sentinel.clone()).await;
        eval_logs::insert_eval_log(&pool, a_owner, 2, None, &sentinel)
            .await
            .unwrap();
        sqlx::query("INSERT INTO eval_logs(commit_id, log_sequence, log_message) VALUES ($1, 1, 'legacy-unowned-private')")
            .bind(f.commit).execute(&pool).await.unwrap();
        let admin: Uuid = sqlx::query_scalar(
            "INSERT INTO users(username, first_name, last_name, email)
             VALUES ($1, 'Log', 'Admin', $2) RETURNING id",
        )
        .bind(format!("admin-{}", Uuid::new_v4()))
        .bind(format!("{}@example.invalid", Uuid::new_v4()))
        .fetch_one(&pool)
        .await
        .unwrap();
        let admin_cookie = cookie(&pool, admin, AuthRole::Admin).await;
        let viewer_cookie = cookie(&pool, f.user, AuthRole::Viewer).await;
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let app = axum::Router::new()
            .route(
                "/api/v1/commits/:commit_id/eval/stream",
                axum::routing::get(handlers::stream_eval_logs),
            )
            .route(
                "/api/v1/commits/:commit_id/eval/logs",
                axum::routing::get(handlers::get_eval_logs_history),
            )
            .with_state(state.clone());
        let _server = TestServer(tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        }));
        let mut old_socket = connect(address, f.commit, &admin_cookie).await;
        let replay_a = replay_barrier(&mut old_socket).await;
        assert_eq!(replay_a.len(), 1);
        assert!(replay_a[0].contains(&sentinel));
        assert_eq!(
            http_logs(address, f.commit, &viewer_cookie)
                .await
                .status()
                .as_u16(),
            404
        );
        let a_http: Vec<handlers::EvalLogEntry> = http_logs(address, f.commit, &admin_cookie)
            .await
            .json()
            .await
            .unwrap();
        assert!(a_http.iter().any(|row| row.message == sentinel));
        assert!(
            !a_http
                .iter()
                .any(|row| row.message.contains("legacy-unowned"))
        );

        // Exercise the real automatic retry and claim functions. Backoff zero is
        // fixture policy, not an elapsed-time assumption in the socket assertions.
        sqlx::query("UPDATE automatic_retry_policy SET max_evaluation_retries = 1, backoff_seconds = 0, transient_only = TRUE WHERE id = 1")
            .execute(&pool).await.unwrap();
        assert_eq!(
            commits::mark_commit_evaluation_failed(
                &pool,
                f.commit,
                "fixture retry",
                a_owner.attempt_number,
                crate::models::retry_policy::RetryFailureClass::Transient
            )
            .await
            .unwrap(),
            commits::EvalFailureOutcome::RetryScheduled
        );
        // Even an admin's old-owner authorization must fail as soon as the
        // queued successor exists, before B binds or has inventory/capture.
        let mut admin_headers = HeaderMap::new();
        admin_headers.insert(axum::http::header::COOKIE, admin_cookie.parse().unwrap());
        assert_eq!(
            handlers::authorize_eval_stream_owner(&state, &admin_headers, f.commit, Some(a_owner))
                .await,
            Err(StatusCode::NOT_FOUND)
        );
        let queued_logs: Vec<handlers::EvalLogEntry> = http_logs(address, f.commit, &admin_cookie)
            .await
            .json()
            .await
            .unwrap();
        assert!(
            queued_logs.is_empty(),
            "queued B cannot inherit A or legacy history"
        );
        assert!(
            handlers::bind_eval_producer(&state, f.commit, a_owner.attempt_number)
                .await
                .is_err()
        );
        assert!(
            handlers::bind_eval_producer(&state, f.commit, a_owner.attempt_number + 1)
                .await
                .is_err()
        );
        assert!(
            matches!(commits::mark_commit_evaluation_started(&pool, f.commit).await.unwrap(),
            commits::EvalStartOutcome::Started { attempt } if attempt == a_owner.attempt_number + 1)
        );
        sqlx::query("DELETE FROM systems WHERE id = $1")
            .bind(f.second_host)
            .execute(&pool)
            .await
            .unwrap();
        let visible_environment: Uuid =
            sqlx::query_scalar("SELECT environment_id FROM systems WHERE id = $1")
                .bind(f.plan.rows[0].system_id)
                .fetch_one(&pool)
                .await
                .unwrap();
        let replacement_name = format!("replacement-{}", Uuid::new_v4());
        let replacement: Uuid = sqlx::query_scalar(
            "INSERT INTO systems(hostname, public_key, derivation, flake_id, environment_id, system_configuration_name)
             VALUES ($1, 'replacement', '', $2, $3, 'alpha') RETURNING id",
        ).bind(&replacement_name).bind(f.flake).bind(visible_environment)
            .fetch_one(&pool).await.unwrap();
        assert_ne!(replacement, f.second_host);
        let b = handlers::bind_eval_producer(&state, f.commit, a_owner.attempt_number + 1)
            .await
            .unwrap();
        let b_owner = b.eval_log_producer.unwrap().owner;
        assert_ne!(b_owner.evaluation_attempt_id, a_owner.evaluation_attempt_id);
        // Rotation drops the sole sender. A receiver-only old socket closes
        // without requiring another producer message or a 20-second heartbeat.
        closed_without_text(&mut old_socket).await;
        assert!(
            handlers::bind_eval_producer(&a, f.commit, b_owner.attempt_number)
                .await
                .is_err()
        );
        let mut b_plan = f.plan.clone();
        b_plan.evaluation_attempt_id = Some(b_owner.evaluation_attempt_id);
        b_plan.attempt_number = b_owner.attempt_number;
        b_plan.rows[1].system_id = Some(replacement);
        b_plan.rows[1].system_name = replacement_name;
        let mut tx = pool.begin().await.unwrap();
        crate::queries::eval_policy_plans::insert_plan(&mut tx, f.commit, &b_plan)
            .await
            .unwrap();
        tx.commit().await.unwrap();
        crate::queries::evaluation_inventory::persist_inventory(
            &pool,
            f.commit,
            b_owner.attempt_number,
            &["alpha".into()],
            &["alpha".into()],
            crate::queries::evaluation_inventory::InventoryExecutionMode::Native,
        )
        .await
        .unwrap();
        assert!(
            allowed(&pool, &f).await,
            "visible replacement and fresh B capture authorize B"
        );

        handlers::broadcast_eval_log(&a, f.commit, format!("{sentinel}-late-log")).await;
        handlers::broadcast_system_status(
            &a,
            f.commit,
            "alpha".into(),
            handlers::SystemEvalStatus::Failed,
            Some(format!("{sentinel}-late-system")),
        )
        .await;
        handlers::broadcast_eval_status(
            &a,
            f.commit,
            "failed".into(),
            Some(format!("{sentinel}-late-status")),
        )
        .await;
        state
            .eval_log_registry
            .lock()
            .await
            .remove(a.eval_log_producer.unwrap());
        assert!(
            eval_logs::insert_eval_log(&pool, a_owner, 3, None, &sentinel)
                .await
                .is_err()
        );
        assert!(
            eval_logs::insert_eval_logs_batch(&pool, a_owner, &[(4, None, sentinel.clone())])
                .await
                .is_err()
        );
        let b_replay = format!("B-replay-{}", Uuid::new_v4());
        handlers::broadcast_eval_log(&b, f.commit, b_replay.clone()).await;
        eval_logs::insert_eval_log(&pool, b_owner, 1, None, &b_replay)
            .await
            .unwrap();
        let mut new_socket = connect(address, f.commit, &viewer_cookie).await;
        let replay_b = replay_barrier(&mut new_socket).await;
        assert_eq!(replay_b.len(), 1);
        assert!(replay_b[0].contains(&b_replay));
        assert!(replay_b.iter().all(|line| !line.contains(&sentinel)));
        let b_live = format!("B-live-{}", Uuid::new_v4());
        handlers::broadcast_eval_log(&b, f.commit, b_live.clone()).await;
        let live = receive_text(&mut new_socket).await;
        assert!(live.contains(&b_live));
        assert!(!live.contains(&sentinel));
        // Same state, foreign UUID: an administrator cannot authorize it either.
        let foreign = eval_logs::EvalLogOwner {
            evaluation_attempt_id: Uuid::new_v4(),
            ..b_owner
        };
        assert_eq!(
            handlers::authorize_eval_stream_owner(&state, &admin_headers, f.commit, Some(foreign))
                .await,
            Err(StatusCode::NOT_FOUND)
        );
        for cookie in [&viewer_cookie, &admin_cookie] {
            let response = http_logs(address, f.commit, cookie).await;
            assert_eq!(response.status().as_u16(), 200);
            let rows: Vec<handlers::EvalLogEntry> = response.json().await.unwrap();
            assert_eq!(rows.len(), 1);
            assert_eq!(rows[0].message, b_replay);
        }
        // Revocation is observed on the next event, not delayed until heartbeat.
        sqlx::query(
            "DELETE FROM user_environment_memberships WHERE user_id = $1 AND environment_id = $2",
        )
        .bind(f.user)
        .bind(visible_environment)
        .execute(&pool)
        .await
        .unwrap();
        handlers::broadcast_eval_log(&b, f.commit, "B-after-revocation".into()).await;
        closed_without_text(&mut new_socket).await;
        assert_eq!(
            http_logs(address, f.commit, &viewer_cookie)
                .await
                .status()
                .as_u16(),
            404
        );
    }
}
