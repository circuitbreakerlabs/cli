use std::{
    collections::VecDeque,
    sync::{Arc, Mutex},
};
use tokio::sync::{OwnedSemaphorePermit, Semaphore};

/// FIFO admission queue backed by a semaphore whose capacity is the hard ceiling.
pub(crate) struct ExecutionQueue<T> {
    semaphore: Arc<Semaphore>,
    concurrency: AdaptiveConcurrency,
    ceiling: usize,
    pending: VecDeque<T>,
}

pub(crate) struct Admitted<T> {
    pub(crate) item: T,
    pub(crate) permit: OwnedSemaphorePermit,
    pub(crate) feedback: ConcurrencyFeedback,
}

#[derive(Clone)]
struct AdaptiveConcurrency {
    state: Arc<Mutex<ConcurrencyState>>,
}

struct ConcurrencyState {
    ceiling: usize,
    current_limit: usize,
    clean_successes: usize,
    window_generation: u64,
    congestion_epoch: u64,
}

/// Per-admission feedback handle. Generations prevent stale completions from
/// growing the window, while epochs collapse concurrent rate limits into one
/// congestion response.
#[derive(Clone)]
pub(crate) struct ConcurrencyFeedback {
    concurrency: AdaptiveConcurrency,
    window_generation: u64,
    congestion_epoch: u64,
}

impl AdaptiveConcurrency {
    fn new(ceiling: usize) -> Self {
        Self {
            state: Arc::new(Mutex::new(ConcurrencyState {
                ceiling,
                current_limit: ceiling,
                clean_successes: 0,
                window_generation: 0,
                congestion_epoch: 0,
            })),
        }
    }

    fn feedback_if_admissible(&self, active: usize) -> Option<ConcurrencyFeedback> {
        let state = self.state.lock().expect("concurrency state poisoned");
        (active <= state.current_limit).then(|| ConcurrencyFeedback {
            concurrency: self.clone(),
            window_generation: state.window_generation,
            congestion_epoch: state.congestion_epoch,
        })
    }

    #[cfg(test)]
    fn current_limit(&self) -> usize {
        self.state
            .lock()
            .expect("concurrency state poisoned")
            .current_limit
    }
}

impl ConcurrencyFeedback {
    pub(crate) fn rate_limited(&self) {
        let adjustment = {
            let mut state = self
                .concurrency
                .state
                .lock()
                .expect("concurrency state poisoned");
            if self.congestion_epoch != state.congestion_epoch {
                return;
            }

            let old_limit = state.current_limit;
            state.current_limit = (state.current_limit / 2).max(1);
            state.clean_successes = 0;
            state.window_generation += 1;
            state.congestion_epoch += 1;
            (old_limit, state.current_limit, state.ceiling)
        };

        let (old_limit, new_limit, ceiling) = adjustment;
        if old_limit == new_limit {
            tracing::debug!(
                current_limit = new_limit,
                ceiling,
                "Concurrency limit remains at minimum after rate limit"
            );
        } else {
            tracing::warn!(
                old_limit,
                new_limit,
                ceiling,
                "Reduced concurrency limit after rate limit"
            );
        }
    }

    pub(crate) fn succeeded(&self) {
        let adjustment = {
            let mut state = self
                .concurrency
                .state
                .lock()
                .expect("concurrency state poisoned");
            if self.window_generation != state.window_generation
                || self.congestion_epoch != state.congestion_epoch
                || state.current_limit == state.ceiling
            {
                return;
            }

            state.clean_successes += 1;
            if state.clean_successes < state.current_limit {
                return;
            }

            let old_limit = state.current_limit;
            state.current_limit += 1;
            state.clean_successes = 0;
            state.window_generation += 1;
            Some((old_limit, state.current_limit, state.ceiling))
        };

        if let Some((old_limit, new_limit, ceiling)) = adjustment {
            tracing::info!(
                old_limit,
                new_limit,
                ceiling,
                "Increased concurrency limit after clean window"
            );
        }
    }
}

impl<T> ExecutionQueue<T> {
    pub(crate) fn new(capacity: u8) -> Self {
        let ceiling = usize::from(capacity);
        Self {
            semaphore: Arc::new(Semaphore::new(ceiling)),
            concurrency: AdaptiveConcurrency::new(ceiling),
            ceiling,
            pending: VecDeque::new(),
        }
    }

    pub(crate) fn push(&mut self, item: T) {
        self.pending.push_back(item);
    }

    #[cfg(test)]
    pub(crate) fn current_limit(&self) -> usize {
        self.concurrency.current_limit()
    }

    pub(crate) fn take_ready(&mut self) -> Option<Admitted<T>> {
        if self.pending.is_empty() {
            return None;
        }
        let permit = self.semaphore.clone().try_acquire_owned().ok()?;
        let active = self.ceiling - self.semaphore.available_permits();
        let Some(feedback) = self.concurrency.feedback_if_admissible(active) else {
            drop(permit);
            return None;
        };
        Some(Admitted {
            item: self
                .pending
                .pop_front()
                .expect("queue was checked as non-empty"),
            permit,
            feedback,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::ExecutionQueue;

    #[test]
    fn admits_in_fifo_order_up_to_capacity() {
        let mut queue = ExecutionQueue::new(2);
        queue.push(1);
        queue.push(2);
        queue.push(3);

        let first = queue.take_ready().unwrap();
        let second = queue.take_ready().unwrap();
        assert_eq!(first.item, 1);
        assert_eq!(second.item, 2);
        assert!(queue.take_ready().is_none());

        drop(first);
        assert_eq!(queue.take_ready().unwrap().item, 3);
    }

    #[test]
    fn rate_limits_halve_once_per_congestion_epoch() {
        let mut queue = ExecutionQueue::new(16);
        for item in 0..16 {
            queue.push(item);
        }
        let admitted = (0..16)
            .map(|_| queue.take_ready().unwrap())
            .collect::<Vec<_>>();

        admitted[0].feedback.rate_limited();
        assert_eq!(queue.concurrency.current_limit(), 8);
        admitted[1].feedback.rate_limited();
        assert_eq!(queue.concurrency.current_limit(), 8);

        drop(admitted);
        queue.push(16);
        let next = queue.take_ready().unwrap();
        next.feedback.rate_limited();
        assert_eq!(queue.concurrency.current_limit(), 4);
    }

    #[test]
    fn multiplicative_decrease_rounds_down_with_a_floor_of_one() {
        let mut queue = ExecutionQueue::new(3);
        queue.push(0);
        let first = queue.take_ready().unwrap();
        first.feedback.rate_limited();
        assert_eq!(queue.current_limit(), 1);
        drop(first);

        queue.push(1);
        let second = queue.take_ready().unwrap();
        second.feedback.rate_limited();
        assert_eq!(queue.current_limit(), 1);
    }

    #[test]
    fn clean_windows_recover_additively_to_the_ceiling() {
        let mut queue = ExecutionQueue::new(4);
        queue.push(0);
        let initial = queue.take_ready().unwrap();
        initial.feedback.rate_limited();
        assert_eq!(queue.concurrency.current_limit(), 2);
        drop(initial);

        for item in 1..=2 {
            queue.push(item);
            let admitted = queue.take_ready().unwrap();
            admitted.feedback.succeeded();
            drop(admitted);
        }
        assert_eq!(queue.concurrency.current_limit(), 3);

        for item in 3..=5 {
            queue.push(item);
            let admitted = queue.take_ready().unwrap();
            admitted.feedback.succeeded();
            drop(admitted);
        }
        assert_eq!(queue.concurrency.current_limit(), 4);
    }

    #[test]
    fn stale_successes_do_not_regrow_a_reduced_window() {
        let mut queue = ExecutionQueue::new(4);
        for item in 0..4 {
            queue.push(item);
        }
        let admitted = (0..4)
            .map(|_| queue.take_ready().unwrap())
            .collect::<Vec<_>>();
        admitted[0].feedback.rate_limited();
        for execution in &admitted {
            execution.feedback.succeeded();
        }
        assert_eq!(queue.concurrency.current_limit(), 2);
    }

    #[test]
    fn shrinking_below_active_count_blocks_replacement_work() {
        let mut queue = ExecutionQueue::new(4);
        for item in 0..5 {
            queue.push(item);
        }
        let mut admitted = (0..4)
            .map(|_| queue.take_ready().unwrap())
            .collect::<Vec<_>>();
        admitted[0].feedback.rate_limited();

        drop(admitted.pop());
        drop(admitted.pop());
        assert!(queue.take_ready().is_none());
        drop(admitted.pop());
        assert_eq!(queue.take_ready().unwrap().item, 4);
    }
}
