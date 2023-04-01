#[macro_export]
macro_rules! create_instance_manager {
    ($manager:ident, $type:ident) => {
        pub trait $type: Any {}

        #[derive(Debug, Default)]
        pub struct $manager {
            cache: FxHashMap<TypeId, Box<dyn Any + Sync + Send>>,
        }

        impl $manager {
            pub fn get<T: Any + Send + Sync + $type>(&self) -> &T {
                if let Some(value) = self.cache.get(&TypeId::of::<T>()) {
                    return value.downcast_ref::<T>().unwrap();
                }

                panic!("{} does not exist!", type_name::<T>())
            }

            pub fn get_mut<T: Any + Send + Sync + $type>(&mut self) -> &mut T {
                if let Some(value) = self.cache.get_mut(&TypeId::of::<T>()) {
                    return value.downcast_mut::<T>().unwrap();
                }

                panic!("{} does not exist!", type_name::<T>())
            }

            pub fn set<T: Any + Send + Sync + $type>(&mut self, instance: T) -> &mut Self {
                self.cache.insert(TypeId::of::<T>(), Box::new(instance));
                self
            }
        }
    };
}
