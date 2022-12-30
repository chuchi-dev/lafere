
use std::pin::Pin;
use std::future::Future;
use std::task::{Poll, Context};

pub struct PinnedFuture<'a, O> {
	inner: Pin<Box<dyn Future<Output=O> + Send + 'a>>
}

impl<'a, O> PinnedFuture<'a, O> {
	pub fn new<F>(future: F) -> Self
	where F: Future<Output=O> + Send + 'a {
		Self {
			inner: Box::pin(future)
		}
	}
}

impl<O> Future for PinnedFuture<'_, O> {
	type Output = O;
	fn poll(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Self::Output> {
		self.get_mut().inner.as_mut().poll(cx)
	}
}

/// Creates a enum with the action type
/// 
/// ## Example
/// ```
/// # use fire_stream_api as stream_api;
/// use stream_api::action;
/// 
/// action! {
/// 	#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
/// 	pub enum Action {
/// 		SomeAction = 1
/// 	}
/// }
/// 
/// // The type `Action::Unkown` will be added, unknown should never be used.
/// ```
#[macro_export]
macro_rules! action {
	(
		IMPL,
		$(#[$attr:meta])*
		($($vis:tt)*) $name:ident {
			$(
				$(#[$variant_attr:meta])*
				$variant:ident = $variant_val:expr
			),*
		}
	) => (
		$(#[$attr])*
		$($vis)* enum $name {
			$(
				$(#[$variant_attr])*
				$variant
			),*,
			Unknown
		}

		impl $crate::message::Action for $name {
			fn empty() -> Self {
				Self::Unknown
			}

			fn from_u16(num: u16) -> Option<Self> {
				match num {
					// use this so nobody uses 0 by accident
					0 => Some(Self::Unknown),
					$(
						$variant_val => Some(Self::$variant)
					),*,
					_ => None
				}
			}

			fn as_u16(&self) -> u16 {
				match self {
					$(
						Self::$variant => $variant_val
					),*,
					Self::Unknown => 0
				}
			}
		}
	);
	($(#[$attr:meta])* pub enum $($toks:tt)*) => (
		$crate::action!( IMPL, $(#[$attr])* (pub) $($toks)* );
	);
	($(#[$attr:meta])* pub ($($vis:tt)+) enum $($toks:tt)*) => (
		$crate::action!( IMPL, $(#[$attr])* (pub ($($vis)+)) $($toks)* );
	);
	($(#[$attr:meta])* enum $($toks:tt)*) => (
		$crate::action!( IMPL, $(#[$attr])* () $($toks)* );
	)
}