use scc::hash_map::OccupiedEntry;
use std::any::{Any, TypeId};
use std::fmt;
use std::ops::{Deref, DerefMut};

pub type BoxedAnyInstance = Box<dyn Any + Sync + Send>;

pub struct InstanceGuard<'i, T> {
    entry: Option<OccupiedEntry<'i, TypeId, BoxedAnyInstance>>,
    marker: std::marker::PhantomData<&'i T>,
}

impl<'i, T: 'static> InstanceGuard<'i, T> {
    pub fn new(entry: OccupiedEntry<'i, TypeId, BoxedAnyInstance>) -> Self {
        InstanceGuard {
            entry: Some(entry),
            marker: std::marker::PhantomData,
        }
    }

    pub fn read(&self) -> &T {
        self.entry
            .as_ref()
            .expect("Instance missing!")
            .get()
            .downcast_ref::<T>()
            .unwrap()
    }

    pub fn write(&mut self) -> &mut T {
        self.entry
            .as_mut()
            .expect("Instance missing!")
            .get_mut()
            .downcast_mut::<T>()
            .unwrap()
    }
}

impl<'i, T: 'static> Deref for InstanceGuard<'i, T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        self.read()
    }
}

impl<'i, T: 'static> DerefMut for InstanceGuard<'i, T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.write()
    }
}

impl<'i, T: fmt::Debug + 'static> fmt::Debug for InstanceGuard<'i, T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}", self.read())
    }
}

impl<'i, T> Drop for InstanceGuard<'i, T> {
    fn drop(&mut self) {
        // This seems necessary to avoid some deadlocks that may occur
        // in our system implementation. I guess scc doesn't drop correctly?
        let entry = self.entry.take();
        drop(entry);
    }
}

// Creates a simple `Any` map registry of objects by type.
// These methods `panic!` because it immediately bubbles up that
// the order of operations for application state is wrong, and
// that systems should be registered correctly.
#[macro_export]
macro_rules! create_instance_manager {
    ($manager:ident, $type:ident) => {
        create_instance_manager!($manager, $type, {
            // No impl
        });
    };
    ($manager:ident, $type:ident, $impl:tt) => {
        pub trait $type: Any $impl

        #[derive(Debug, Default)]
        pub struct $manager {
            cache: scc::HashMap<TypeId, $crate::BoxedAnyInstance>,
        }

        impl $manager {
            /// Get an instance reference for the provided type.
            /// If the instance does not exist, a panic will be triggered.
            pub fn get<T: Any + Send + Sync + $type>(&self) -> $crate::InstanceGuard<T> {
                if let Some(entry) = self.cache.get(&TypeId::of::<T>()) {
                    return $crate::InstanceGuard::new(entry);
                }

                panic!("{} does not exist!", type_name::<T>())
            }

            /// Get an instance reference for the provided type asynchronously.
            /// If the instance does not exist, a panic will be triggered.
            pub async fn get_async<T: Any + Send + Sync + $type>(&self) -> $crate::InstanceGuard<T> {
                if let Some(entry) = self.cache.get_async(&TypeId::of::<T>()).await {
                    return $crate::InstanceGuard::new(entry);
                }

                panic!("{} does not exist!", type_name::<T>())
            }

            /// Set the instance into the registry with the provided type.
            /// If an exact type already exists, it'll be overwritten.
            pub fn set<T: Any + Send + Sync + $type>(&self, instance: T) {
               let _ = self.cache.insert(TypeId::of::<T>(), Box::new(instance));
            }

            /// Set the instance into the registry with the provided type asynchronously.
            /// If an exact type already exists, it'll be overwritten.
            pub async fn set_async<T: Any + Send + Sync + $type>(&self, instance: T) {
               let _ = self.cache.insert_async(TypeId::of::<T>(), Box::new(instance)).await;
            }
        }
    };
}
