use std::cell::RefCell;
use std::sync::atomic::Ordering;
use std::sync::Arc;

#[derive(Debug)]
struct Node<T> {
    elem: Option<T>,
    next: Option<Arc<RefCell<Node<T>>>>,
}

// Single producer, single consumer lock-free queue.
// The producer only updates the tail, and the consumer only updates the head,
// then we do not need a lock to protect the queue.
// The arc is unnecessary in theory, as we can release the node when it has been consumed,
// but doing this requires to fight with Rust's safety system which is hard.

// NOTICE: this queue is used for single producer and single consumer only.
#[derive(Debug)]
struct Queue<T> {
    head: Arc<RefCell<Node<T>>>,
    tail: Arc<RefCell<Node<T>>>,
}

impl<T> Queue<T> {
    fn new() -> Self {
        let node = Arc::new(RefCell::new(Node {
            elem: None,
            next: None,
        }));

        Queue {
            head: Arc::clone(&node),
            tail: node,
        }
    }

    fn produce(&mut self, elem: Option<T>) {
        // Create new node first
        let new_node = Arc::new(RefCell::new(Node {
            elem: None,
            next: None,
        }));

        self.tail.borrow_mut().elem = elem;

        self.tail.borrow_mut().next = Some(Arc::clone(&new_node));

        // Memory barrier to ensure previous writes happen before the tail pointer is updated
        std::sync::atomic::fence(Ordering::Release);
        self.tail = new_node;
    }

    fn consume(&mut self) -> Option<T> {
        // Need need memory barrier, only consumer can update the head pointer,
        // it's ok if the consumer can't see the producer's updates, it can cause empty returns false result,
        // but as consumer consumes the queue in while loop, it will see the producer's updates eventually.
        // For the producer, it only depends on the head, then consumer doesn't impact it, so it fine without any memory barrier.
        if self.empty() {
            return None;
        }

        let elem = self.head.borrow_mut().elem.take();
        let next = self.head.borrow().next.as_ref().map(|n| Arc::clone(n));
        self.head = next.unwrap();

        elem
    }

    fn empty(&self) -> bool {
        Arc::ptr_eq(&self.head, &self.tail)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_list() {
        let list: Queue<i32> = Queue::new();
        assert!(list.head.borrow().elem.is_none());
        assert!(list.head.borrow().next.is_none());
        assert!(list.empty());
    }

    #[test]
    fn test_push_and_pop() {
        let mut list = Queue::new();

        assert_eq!(list.consume(), None);

        list.produce(Some(1));
        list.produce(Some(2));
        list.produce(Some(3));

        assert_eq!(list.consume(), Some(1));
        assert_eq!(list.consume(), Some(2));
        assert_eq!(list.consume(), Some(3));
        assert_eq!(list.consume(), None);
    }
}
