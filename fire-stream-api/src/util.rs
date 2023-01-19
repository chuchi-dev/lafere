use crate::server::{Session, Data};

use std::any::{Any, TypeId};
use std::cell::RefCell;
use std::mem::ManuallyDrop;

pub use stream::util::PinnedFuture;


fn is_req<T: Any, R: Any>() -> bool {
	TypeId::of::<T>() == TypeId::of::<R>()
}

fn is_session<T: Any>() -> bool {
	TypeId::of::<T>() == TypeId::of::<Session>()
}

fn is_data<T: Any>() -> bool {
	TypeId::of::<T>() == TypeId::of::<Data>()
}

/// fn to check if a type can be accessed in a route as reference
#[doc(hidden)]
#[inline]
pub fn valid_data_as_ref<T: Any, R: Any>(data: &Data) -> bool {
	is_req::<T, R>() || is_session::<T>() ||
	is_data::<T>() || data.exists::<T>()
}

/// fn to check if a type can be accessed in a route as mutable reference
#[doc(hidden)]
#[inline]
pub fn valid_data_as_owned<T: Any, R: Any>(_data: &Data) -> bool {
	is_req::<T, R>()
}

#[doc(hidden)]
pub struct DataManager<T> {
	inner: RefCell<Option<T>>
}

impl<T> DataManager<T> {
	pub fn new(val: T) -> Self {
		Self {
			inner: RefCell::new(Some(val))
		}
	}

	/// ## Panics
	/// if the value is already taken or borrowed
	#[inline]
	pub fn take(&self) -> T {
		self.inner.borrow_mut().take().unwrap()
	}

	/// ## Panics
	/// If the values is already taken or borrowed mutably
	#[inline]
	pub fn as_ref(&self) -> &T {
		let r = self.inner.borrow();
		let r = ManuallyDrop::new(r);
		// since the borrow counter does not get decreased because of the
		// ManuallyDrop and the lifetime not getting expanded this is safe
		unsafe {
			&*(&**r as *const Option<T>)
		}.as_ref().unwrap()
	}

	/// ##Panics
	/// if the value was taken previously
	#[inline]
	pub fn take_owned(mut self) -> T {
		self.inner.get_mut().take().unwrap()
	}
}

#[doc(hidden)]
#[inline]
pub fn get_data_as_ref<'a, T: Any, R: Any>(
	data: &'a Data,
	session: &'a Session,
	req: &'a DataManager<R>
) -> &'a T {
	if is_req::<T, R>() {
		let req = req.as_ref();
		<dyn Any>::downcast_ref(req).unwrap()
	} else if is_session::<T>() {
		<dyn Any>::downcast_ref(session).unwrap()
	} else if is_data::<T>() {
		<dyn Any>::downcast_ref(data).unwrap()
	} else {
		data.get::<T>().unwrap()
	}
}

#[doc(hidden)]
#[inline]
pub fn get_data_as_owned<T: Any, R: Any>(
	_data: &Data,
	_session: &Session,
	req: &DataManager<R>
) -> T {
	if is_req::<T, R>() {
		let req = req.take();
		unsafe {
			transform_owned::<T, R>(req)
		}
	} else {
		unreachable!()
	}
}

/// Safety you need to know that T is `R`
#[doc(hidden)]
#[inline]
pub(crate) unsafe fn transform_owned<T: Any + Sized, R: Any>(from: R) -> T {
	let mut from = ManuallyDrop::new(from);
	(&mut from as *mut ManuallyDrop<R> as *mut T).read()
}