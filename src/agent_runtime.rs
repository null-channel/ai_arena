use std::sync::{
    Arc,
    atomic::{AtomicU64, Ordering},
};
use std::time::Duration;

use tokio::sync::Semaphore;

use crate::agent::{AgentError, AgentResult, GameAgent, MoveRequest, MoveResponse};

pub struct ManagedAgent<A> {
    inner: A,
    request_timeout: Duration,
    max_retries: u32,
    retry_backoff: Duration,
    max_total_tokens: Option<u64>,
    consumed_tokens: AtomicU64,
    concurrency_limiter: Arc<Semaphore>,
}

impl<A> ManagedAgent<A> {
    pub fn new(
        inner: A,
        request_timeout: Duration,
        max_retries: u32,
        retry_backoff: Duration,
        max_total_tokens: Option<u64>,
        concurrency_limiter: Arc<Semaphore>,
    ) -> Self {
        Self {
            inner,
            request_timeout,
            max_retries,
            retry_backoff,
            max_total_tokens,
            consumed_tokens: AtomicU64::new(0),
            concurrency_limiter,
        }
    }

    fn record_usage(&self, response: &MoveResponse) -> AgentResult<()> {
        let Some(limit) = self.max_total_tokens else {
            return Ok(());
        };
        let Some(usage) = response.token_usage else {
            return Err(AgentError::BudgetExceeded(
                "provider did not report token usage".into(),
                None,
            ));
        };

        loop {
            let consumed = self.consumed_tokens.load(Ordering::Relaxed);
            let next = consumed.checked_add(usage.total_tokens);
            let (updated, exceeded) = match next {
                Some(next) if next <= limit => (next, false),
                Some(_) | None => (limit, true),
            };
            if self
                .consumed_tokens
                .compare_exchange_weak(consumed, updated, Ordering::Relaxed, Ordering::Relaxed)
                .is_ok()
            {
                return if exceeded {
                    Err(AgentError::BudgetExceeded(
                        format!(
                            "token limit {limit} would be exceeded (already consumed {consumed}, response used {})",
                            usage.total_tokens
                        ),
                        Some(usage),
                    ))
                } else {
                    Ok(())
                };
            }
        }
    }

    async fn retry_delay(&self, attempt: u32) {
        let multiplier = 1_u32.checked_shl(attempt).unwrap_or(u32::MAX);
        let delay = self.retry_backoff.saturating_mul(multiplier);
        if !delay.is_zero() {
            tokio::time::sleep(delay).await;
        }
    }
}

#[async_trait::async_trait]
impl<A: GameAgent> GameAgent for ManagedAgent<A> {
    fn name(&self) -> &str {
        self.inner.name()
    }

    async fn execute_turn(&self, request: &MoveRequest) -> AgentResult<MoveResponse> {
        if let Some(limit) = self.max_total_tokens
            && self.consumed_tokens.load(Ordering::Relaxed) >= limit
        {
            return Err(AgentError::BudgetExceeded(
                format!("token limit {limit} has been reached"),
                None,
            ));
        }

        let _permit = self
            .concurrency_limiter
            .acquire()
            .await
            .map_err(|_| AgentError::Internal("request concurrency limiter closed".into()))?;

        for attempt in 0..=self.max_retries {
            match tokio::time::timeout(self.request_timeout, self.inner.execute_turn(request)).await
            {
                Ok(Ok(response)) => {
                    self.record_usage(&response)?;
                    return Ok(response);
                }
                Ok(Err(
                    error @ (AgentError::InvalidRequest(_) | AgentError::InvalidResponse(_)),
                )) => {
                    return Err(error);
                }
                Ok(Err(error)) if attempt == self.max_retries => return Err(error),
                Err(_) if attempt == self.max_retries => {
                    return Err(AgentError::Timeout {
                        timeout_ms: self.request_timeout.as_millis() as u64,
                    });
                }
                Ok(Err(_)) | Err(_) => self.retry_delay(attempt).await,
            }
        }

        unreachable!("retry loop always returns on its final attempt")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::TokenUsage;
    use serde_json::json;
    use std::sync::atomic::{AtomicU32, AtomicUsize};

    struct TestAgent {
        calls: Arc<AtomicU32>,
        transient_failures: u32,
        delay: Duration,
        usage: Option<TokenUsage>,
    }

    #[async_trait::async_trait]
    impl GameAgent for TestAgent {
        fn name(&self) -> &str {
            "test"
        }

        async fn execute_turn(&self, _request: &MoveRequest) -> AgentResult<MoveResponse> {
            let call = self.calls.fetch_add(1, Ordering::Relaxed);
            if !self.delay.is_zero() {
                tokio::time::sleep(self.delay).await;
            }
            if call < self.transient_failures {
                return Err(AgentError::Internal("transient".into()));
            }
            Ok(MoveResponse {
                chosen_move: json!({"move": 1}),
                diagnostics: None,
                token_usage: self.usage,
            })
        }
    }

    fn request() -> MoveRequest {
        MoveRequest {
            turn_index: 0,
            game_id: "test".into(),
            state: json!({}),
            expected_move_schema: json!({}),
        }
    }

    fn managed(
        inner: TestAgent,
        timeout: Duration,
        retries: u32,
        budget: Option<u64>,
    ) -> ManagedAgent<TestAgent> {
        ManagedAgent::new(
            inner,
            timeout,
            retries,
            Duration::ZERO,
            budget,
            Arc::new(Semaphore::new(2)),
        )
    }

    #[tokio::test]
    async fn retries_transient_errors() {
        let calls = Arc::new(AtomicU32::new(0));
        let agent = managed(
            TestAgent {
                calls: Arc::clone(&calls),
                transient_failures: 2,
                delay: Duration::ZERO,
                usage: None,
            },
            Duration::from_millis(100),
            2,
            None,
        );

        assert!(agent.execute_turn(&request()).await.is_ok());
        assert_eq!(calls.load(Ordering::Relaxed), 3);
    }

    #[tokio::test]
    async fn times_out_after_the_configured_retries() {
        let calls = Arc::new(AtomicU32::new(0));
        let agent = managed(
            TestAgent {
                calls: Arc::clone(&calls),
                transient_failures: 0,
                delay: Duration::from_millis(20),
                usage: None,
            },
            Duration::from_millis(1),
            1,
            None,
        );

        assert!(matches!(
            agent.execute_turn(&request()).await,
            Err(AgentError::Timeout { timeout_ms: 1 })
        ));
        assert_eq!(calls.load(Ordering::Relaxed), 2);
    }

    #[tokio::test]
    async fn stops_after_the_token_budget_is_exhausted() {
        let calls = Arc::new(AtomicU32::new(0));
        let agent = managed(
            TestAgent {
                calls: Arc::clone(&calls),
                transient_failures: 0,
                delay: Duration::ZERO,
                usage: Some(TokenUsage {
                    input_tokens: 3,
                    output_tokens: 3,
                    total_tokens: 6,
                }),
            },
            Duration::from_millis(100),
            0,
            Some(10),
        );

        assert!(agent.execute_turn(&request()).await.is_ok());
        assert!(matches!(
            agent.execute_turn(&request()).await,
            Err(AgentError::BudgetExceeded(_, Some(_)))
        ));
        assert!(matches!(
            agent.execute_turn(&request()).await,
            Err(AgentError::BudgetExceeded(_, None))
        ));
        assert_eq!(calls.load(Ordering::Relaxed), 2);
    }

    struct ConcurrencyAgent {
        active: Arc<AtomicUsize>,
        max_active: Arc<AtomicUsize>,
    }

    #[async_trait::async_trait]
    impl GameAgent for ConcurrencyAgent {
        fn name(&self) -> &str {
            "concurrency-test"
        }

        async fn execute_turn(&self, _request: &MoveRequest) -> AgentResult<MoveResponse> {
            let active = self.active.fetch_add(1, Ordering::SeqCst) + 1;
            self.max_active.fetch_max(active, Ordering::SeqCst);
            tokio::time::sleep(Duration::from_millis(5)).await;
            self.active.fetch_sub(1, Ordering::SeqCst);
            Ok(MoveResponse {
                chosen_move: json!({}),
                diagnostics: None,
                token_usage: None,
            })
        }
    }

    #[tokio::test]
    async fn shares_the_match_concurrency_limit() {
        let active = Arc::new(AtomicUsize::new(0));
        let max_active = Arc::new(AtomicUsize::new(0));
        let limiter = Arc::new(Semaphore::new(1));
        let create = || {
            ManagedAgent::new(
                ConcurrencyAgent {
                    active: Arc::clone(&active),
                    max_active: Arc::clone(&max_active),
                },
                Duration::from_millis(100),
                0,
                Duration::ZERO,
                None,
                Arc::clone(&limiter),
            )
        };
        let first = create();
        let second = create();
        let request = request();

        let (first_result, second_result) =
            tokio::join!(first.execute_turn(&request), second.execute_turn(&request));

        assert!(first_result.is_ok());
        assert!(second_result.is_ok());
        assert_eq!(max_active.load(Ordering::SeqCst), 1);
    }
}
