use std::collections::VecDeque;
use std::sync::Mutex;

pub struct MessageQueue<T: Clone> {
    deque: Mutex<VecDeque<T>>,
}

impl<T: Clone> Default for MessageQueue<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T: Clone> MessageQueue<T> {
    pub fn new() -> Self {
        Self {
            deque: Mutex::new(VecDeque::new()),
        }
    }

    pub fn push_front(&mut self, elem: T) {
        let mut deque = self.deque.lock().unwrap();
        deque.push_front(elem);
    }

    pub fn push_back(&mut self, elem: T) {
        let mut deque = self.deque.lock().unwrap();
        deque.push_back(elem);
    }

    pub fn front(&self) -> Option<T> {
        let deque = self.deque.lock().unwrap();

        deque.front().cloned()
    }

    pub fn back(&self) -> Option<T> {
        let deque = self.deque.lock().unwrap();

        deque.back().cloned()
    }

    pub fn pop_front(&mut self) -> Option<T> {
        let mut deque = self.deque.lock().unwrap();

        deque.pop_front()
    }

    pub fn pop_back(&mut self) -> Option<T> {
        let mut deque = self.deque.lock().unwrap();

        deque.pop_back()
    }

    pub fn is_empty(&self) -> bool {
        let deque = self.deque.lock().unwrap();

        deque.is_empty()
    }

    pub fn clear(&mut self) {
        let mut deque = self.deque.lock().unwrap();
        deque.clear();
    }

    pub fn len(&self) -> usize {
        let deque = self.deque.lock().unwrap();

        deque.len()
    }
}
