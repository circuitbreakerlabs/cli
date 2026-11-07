use std::{collections::VecDeque, sync::Arc};
use tokio::sync::{OwnedSemaphorePermit, Semaphore};

/// FIFO admission queue backed by a fixed-capacity semaphore.
pub(crate) struct ExecutionQueue<T> {
    semaphore: Arc<Semaphore>,
    pending: VecDeque<T>,
}

pub(crate) struct Admitted<T> {
    pub(crate) item: T,
    pub(crate) permit: OwnedSemaphorePermit,
}

impl<T> ExecutionQueue<T> {
    pub(crate) fn new(capacity: u8) -> Self {
        Self {
            semaphore: Arc::new(Semaphore::new(usize::from(capacity))),
            pending: VecDeque::new(),
        }
    }

    pub(crate) fn push(&mut self, item: T) {
        self.pending.push_back(item);
    }

    pub(crate) fn take_ready(&mut self) -> Option<Admitted<T>> {
        if self.pending.is_empty() {
            return None;
        }
        let permit = self.semaphore.clone().try_acquire_owned().ok()?;
        Some(Admitted {
            item: self
                .pending
                .pop_front()
                .expect("queue was checked as non-empty"),
            permit,
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
}
