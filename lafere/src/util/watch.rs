use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, RwLock};

use tokio::sync::Notify;

pub fn channel<T>(data: T) -> (Sender<T>, Receiver<T>) {
	let shared = Arc::new(Shared {
		data: RwLock::new(data),
		version: AtomicUsize::new(1),
		tx_count: AtomicUsize::new(1),
		notify: Notify::new(),
	});

	(
		Sender {
			inner: shared.clone(),
		},
		Receiver {
			inner: shared,
			version: 0,
		},
	)
}

#[derive(Debug)]
pub struct Sender<T> {
	inner: Arc<Shared<T>>,
}

impl<T> Clone for Sender<T> {
	fn clone(&self) -> Self {
		let inner = self.inner.clone();
		// relaxed since this is only a counter
		inner.tx_count.fetch_add(1, Ordering::Relaxed);

		Self { inner }
	}
}

impl<T> Drop for Sender<T> {
	fn drop(&mut self) {
		// relaxed since this is only a counter
		let prev_count = self.inner.tx_count.fetch_sub(1, Ordering::Relaxed);
		if prev_count == 1 {
			// we are the last sender
			// notify receivers
			self.inner.notify.notify_waiters();
		}
	}
}

impl<T> Sender<T> {
	/// It is possible that there are no receivers left.
	///
	/// This is not checked
	pub fn send(&self, data: T) {
		{
			let mut lock = self.inner.data.write().unwrap();
			*lock = data;
			self.inner.version.fetch_add(1, Ordering::SeqCst);
		}
		self.inner.notify.notify_waiters();
	}

	pub fn newest(&self) -> T
	where
		T: Clone,
	{
		self.inner.data.read().unwrap().clone()
	}
}

#[derive(Debug)]
pub struct Receiver<T> {
	inner: Arc<Shared<T>>,
	version: usize,
}

impl<T> Clone for Receiver<T> {
	fn clone(&self) -> Self {
		Self {
			inner: self.inner.clone(),
			version: self.version,
		}
	}
}

impl<T> Receiver<T> {
	/// Returns None if there isn't any sender left.
	pub async fn recv(&mut self) -> Option<T>
	where
		T: Clone,
	{
		loop {
			// let get the notification before we check if there exists a new
			// version to not miss any notification that could be sent
			// between our check.
			let noti = self.inner.notify.notified();

			let n_version = self.inner.version.load(Ordering::SeqCst);
			if self.version != n_version {
				self.version = n_version;
				return Some(self.inner.data.read().unwrap().clone());
			}

			// todo: does this need to be SeqCst?
			let tx_count = self.inner.tx_count.load(Ordering::SeqCst);
			if tx_count == 0 {
				return None;
			}

			noti.await;
		}
	}

	#[allow(dead_code)]
	pub fn newest(&self) -> T
	where
		T: Clone,
	{
		self.inner.data.read().unwrap().clone()
	}
}

/// does not track if there are any receivers left
#[derive(Debug)]
pub struct Shared<T> {
	data: RwLock<T>,
	version: AtomicUsize,
	tx_count: AtomicUsize,
	notify: Notify,
}

#[cfg(test)]
mod tests {

	use super::*;
	use tokio::time::{Duration, sleep};

	#[tokio::test]
	async fn test_wakeup() {
		let (tx, mut rx) = channel(true);

		let task = tokio::spawn(async move {
			let val = rx.recv().await.unwrap();
			assert_eq!(val, true);
			let n_val = rx.recv().await.unwrap();
			assert_eq!(n_val, false);

			assert!(rx.recv().await.is_none())
		});

		// wait for the task to start running
		sleep(Duration::from_millis(100)).await;

		tx.send(false);

		drop(tx);

		task.await.unwrap();
	}
}
