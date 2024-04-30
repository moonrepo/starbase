use scc::hash_map::OccupiedEntry;
use std::any::{Any, TypeId};
use std::ops::{Deref, DerefMut};

pub type BoxedAnyInstance = Box<dyn Any + Sync + Send>;

pub struct InstanceGuard<'i, T> {
    entry: OccupiedEntry<'i, TypeId, BoxedAnyInstance>,
    marker: std::marker::PhantomData<&'i T>,
}

impl<'i, T: 'static> InstanceGuard<'i, T> {
    pub fn new(entry: OccupiedEntry<'i, TypeId, BoxedAnyInstance>) -> Self {
        InstanceGuard {
            entry,
            marker: std::marker::PhantomData,
        }
    }

    pub fn read(&self) -> &T {
        self.entry.get().downcast_ref::<T>().unwrap()
    }

    pub fn write(&mut self) -> &mut T {
        self.entry.get_mut().downcast_mut::<T>().unwrap()
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
            pub fn get<T: Any + Send + Sync + $type>(&self) -> crate::InstanceGuard<T> {
                if let Some(entry) = self.cache.get(&TypeId::of::<T>()) {
                    return crate::InstanceGuard::new(entry);
                }

                panic!("{} does not exist!", type_name::<T>())
            }

            /// Set the instance into the registry with the provided type.
            /// If an exact type already exists, it'll be overwritten.
            pub fn set<T: Any + Send + Sync + $type>(&self, instance: T) {
               let _ = self.cache.insert(TypeId::of::<T>(), Box::new(instance));
            }
        }
    };
}
