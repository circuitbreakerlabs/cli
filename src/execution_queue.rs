use std::{collections::VecDeque, sync::Arc};
use tokio::sync::{OwnedSemaphorePermit, Semaphore};

pub(crate) struct Admitted<T> {
    pub(crate) item: T,
    pub(crate) permit: OwnedSemaphorePermit,
}

/// A FIFO queue that admits work while semaphore capacity is available.
pub(crate) struct ExecutionQueue<T> {
    semaphore: Arc<Semaphore>,
    pending: VecDeque<T>,
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
        let item = self
            .pending
            .pop_front()
            .expect("a permit is only requested while queued work exists");
        Some(Admitted { item, permit })
    }

    pub(crate) fn remove_pending(&mut self, predicate: impl FnMut(&T) -> bool) -> Option<T> {
        let index = self.pending.iter().position(predicate)?;
        self.pending.remove(index)
    }

    pub(crate) fn len(&self) -> usize {
        self.pending.len()
    }
}

#[cfg(test)]
mod tests {
    use super::ExecutionQueue;

    #[test]
    fn admits_in_fifo_order_as_capacity_is_released() {
        let mut queue = ExecutionQueue::new(2);
        queue.push(1);
        queue.push(2);
        queue.push(3);

        let first = queue.take_ready().unwrap();
        let second = queue.take_ready().unwrap();
        assert_eq!(first.item, 1);
        assert_eq!(second.item, 2);
        assert!(queue.take_ready().is_none());

        drop(second.permit);
        let third = queue.take_ready().unwrap();
        assert_eq!(third.item, 3);
        assert_eq!(queue.len(), 0);

        drop(first.permit);
        drop(third.permit);
    }

    #[test]
    fn removes_pending_work_without_disturbing_fifo_order() {
        let mut queue = ExecutionQueue::new(1);
        queue.push((1, "first"));
        queue.push((2, "cancelled"));
        queue.push((3, "last"));

        let first = queue.take_ready().unwrap();
        assert_eq!(
            queue.remove_pending(|(id, _)| *id == 2),
            Some((2, "cancelled"))
        );
        drop(first.permit);

        assert_eq!(queue.take_ready().unwrap().item, (3, "last"));
    }
}
