use serde::Serialize;

pub const ALLOWED_RETRY_BACKOFF_SECONDS: [i32; 6] = [0, 10, 30, 60, 120, 300];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RetryFailureClass {
    Transient,
    Deterministic,
    Authorization,
    Cancelled,
    DerivationMismatch,
    Unknown,
}

pub fn automatic_retry_eligible(transient_only: bool, class: RetryFailureClass) -> bool {
    match class {
        RetryFailureClass::Authorization
        | RetryFailureClass::Cancelled
        | RetryFailureClass::DerivationMismatch => false,
        RetryFailureClass::Transient => true,
        RetryFailureClass::Deterministic | RetryFailureClass::Unknown => !transient_only,
    }
}

pub fn automatic_retry_budget_remaining(used_retries: i32, max_retries: i32) -> bool {
    max_retries > 0 && used_retries < max_retries
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, sqlx::FromRow)]
pub struct AutomaticRetryPolicy {
    pub max_build_retries: i16,
    pub max_evaluation_retries: i16,
    pub backoff_seconds: i32,
    pub transient_only: bool,
}

impl Default for AutomaticRetryPolicy {
    fn default() -> Self {
        Self {
            max_build_retries: 2,
            max_evaluation_retries: 1,
            backoff_seconds: 30,
            transient_only: true,
        }
    }
}

impl AutomaticRetryPolicy {
    pub fn validate(&self) -> Result<(), Vec<RetryPolicyValidationError>> {
        let mut errors = Vec::new();

        if !(0..=5).contains(&self.max_build_retries) {
            errors.push(RetryPolicyValidationError {
                field: "max_build_retries",
                message: "must be between 0 and 5",
            });
        }
        if !(0..=5).contains(&self.max_evaluation_retries) {
            errors.push(RetryPolicyValidationError {
                field: "max_evaluation_retries",
                message: "must be between 0 and 5",
            });
        }
        if !ALLOWED_RETRY_BACKOFF_SECONDS.contains(&self.backoff_seconds) {
            errors.push(RetryPolicyValidationError {
                field: "backoff_seconds",
                message: "must be one of 0, 10, 30, 60, 120, or 300",
            });
        }

        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct RetryPolicyValidationError {
    pub field: &'static str,
    pub message: &'static str,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_match_server_policy_defaults() {
        assert_eq!(
            AutomaticRetryPolicy::default(),
            AutomaticRetryPolicy {
                max_build_retries: 2,
                max_evaluation_retries: 1,
                backoff_seconds: 30,
                transient_only: true,
            }
        );
    }

    #[test]
    fn validation_accepts_all_boundaries_and_backoffs() {
        for backoff_seconds in ALLOWED_RETRY_BACKOFF_SECONDS {
            let policy = AutomaticRetryPolicy {
                max_build_retries: 0,
                max_evaluation_retries: 5,
                backoff_seconds,
                transient_only: false,
            };
            assert_eq!(policy.validate(), Ok(()));
        }
    }

    #[test]
    fn validation_reports_every_invalid_field() {
        let policy = AutomaticRetryPolicy {
            max_build_retries: -1,
            max_evaluation_retries: 6,
            backoff_seconds: 20,
            transient_only: true,
        };

        let errors = policy.validate().expect_err("policy must be invalid");
        assert_eq!(errors.len(), 3);
        assert_eq!(errors[0].field, "max_build_retries");
        assert_eq!(errors[1].field, "max_evaluation_retries");
        assert_eq!(errors[2].field, "backoff_seconds");
    }

    #[test]
    fn retry_eligibility_matrix_is_fail_closed() {
        for transient_only in [true, false] {
            for (class, expected) in [
                (RetryFailureClass::Transient, true),
                (RetryFailureClass::Deterministic, !transient_only),
                (RetryFailureClass::Unknown, !transient_only),
                (RetryFailureClass::Authorization, false),
                (RetryFailureClass::Cancelled, false),
                (RetryFailureClass::DerivationMismatch, false),
            ] {
                assert_eq!(automatic_retry_eligible(transient_only, class), expected);
            }
        }
    }

    #[test]
    fn retry_budget_zero_and_exhausted_do_not_allow_an_attempt() {
        assert!(!automatic_retry_budget_remaining(0, 0));
        assert!(automatic_retry_budget_remaining(0, 1));
        assert!(!automatic_retry_budget_remaining(1, 1));
        assert!(!automatic_retry_budget_remaining(2, 1));
    }
}
