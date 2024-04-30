use scc::hash_map::OccupiedEntry;
use std::any::{Any, TypeId};
use std::ops::{Deref, DerefMut};

pub type BoxedAnyInstance = Box<dyn Any + Sync + Send>;

pub struct InstanceReadGuard<'i, T> {
    pub value: &'i T,
    entry: OccupiedEntry<'i, TypeId, BoxedAnyInstance>,
}

impl<'i, T> InstanceReadGuard<'i, T> {
    pub fn new(entry: OccupiedEntry<'i, TypeId, BoxedAnyInstance>) -> Self {
        InstanceReadGuard {
            value: entry.get().downcast_ref::<T>().unwrap(),
            entry,
        }
    }
}

impl<'i, T> Deref for InstanceReadGuard<'i, T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        self.value
    }
}

pub struct InstanceWriteGuard<'i, T> {
    pub value: &'i mut T,
    entry: OccupiedEntry<'i, TypeId, BoxedAnyInstance>,
}

impl<'i, T> InstanceWriteGuard<'i, T> {
    pub fn new(mut entry: OccupiedEntry<'i, TypeId, BoxedAnyInstance>) -> Self {
        InstanceWriteGuard {
            value: entry.get_mut().downcast_mut::<T>().unwrap(),
            entry,
        }
    }
}

impl<'i, T> Deref for InstanceWriteGuard<'i, T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        self.value
    }
}

impl<'i, T> DerefMut for InstanceWriteGuard<'i, T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.value
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
            cache: scc::HashMap<TypeId, crate::BoxedAnyInstance>,
        }

        impl $manager {
            /// Get an immutable instance reference for the provided type.
            /// If the instance does not exist, a panic will be triggered.
            pub fn get<T: Any + Send + Sync + $type>(&self) -> crate::InstanceReadGuard<T> {
                if let Some(entry) = self.cache.get(&TypeId::of::<T>()) {
                    return crate::InstanceReadGuard::new(entry);
                }

                panic!("{} does not exist!", type_name::<T>())
            }

            /// Get a mutable instance reference for the provided type.
            /// If the instance does not exist, a panic will be triggered.
            pub fn get_mut<T: Any + Send + Sync + $type>(&self) -> crate::InstanceWriteGuard<T> {
                if let Some(entry) = self.cache.get(&TypeId::of::<T>()) {
                    return crate::InstanceWriteGuard::new(entry);
                }

                panic!("{} does not exist!", type_name::<T>())
            }

            /// Set the instance into the registry with the provided type.
            /// If an exact type already exists, it'll be overwritten.
            pub fn set<T: Any + Send + Sync + $type>(&self, instance: T) {
                self.cache.insert(TypeId::of::<T>(), Box::new(instance));
            }
        }
    };
}
