// Creates a simple `Any` map registry of objects by type.
// These methods `panic!` because it immediately bubbles up that
// the order of operations for application state is wrong, and
// that systems should be registered correctly.
#[macro_export]
macro_rules! create_instance_manager {
    ($manager:ident, $type:ident) => {
        pub trait $type: Any {}

        #[derive(Debug, Default)]
        pub struct $manager {
            cache: FxHashMap<TypeId, Box<dyn Any + Sync + Send>>,
        }

        impl $manager {
            /// Get an immutable instance reference for the provided type.
            /// If the instance does not exist, a panic will be triggered.
            pub fn get<T: Any + Send + Sync + $type>(&self) -> &T {
                if let Some(value) = self.cache.get(&TypeId::of::<T>()) {
                    return value.downcast_ref::<T>().unwrap();
                }

                panic!("{} does not exist!", type_name::<T>())
            }

            /// Get a mutable instance reference for the provided type.
            /// If the instance does not exist, a panic will be triggered.
            pub fn get_mut<T: Any + Send + Sync + $type>(&mut self) -> &mut T {
                if let Some(value) = self.cache.get_mut(&TypeId::of::<T>()) {
                    return value.downcast_mut::<T>().unwrap();
                }

                panic!("{} does not exist!", type_name::<T>())
            }

            /// Set the instance into the registry with the provided type.
            /// If an exact type already exists, it'll be overwritten.
            pub fn set<T: Any + Send + Sync + $type>(&mut self, instance: T) -> &mut Self {
                self.cache.insert(TypeId::of::<T>(), Box::new(instance));
                self
            }
        }
    };
}
