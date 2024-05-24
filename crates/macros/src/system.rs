use darling::export::NestedMeta;
use darling::FromMeta;
use proc_macro::TokenStream;
use quote::{format_ident, quote};
use std::collections::BTreeMap;
use syn::{parse_macro_input, FnArg, GenericArgument, Ident, Pat, PathArguments, Type};

enum SystemParam<'a> {
    // ManagerMut,
    // ManagerRef,
    ParamMut(&'a Type),
    ParamRaw(&'a Type),
    ParamRef(&'a Type),
    ParamRefWithExtract(&'a Type, &'a Type),
    // Special case
    ArgsRef(&'a Type),
}

enum InstanceType {
    Emitter,
    Resource,
    State,
}

impl InstanceType {
    pub fn manager_name(&self) -> &str {
        match self {
            InstanceType::Emitter => "Emitters",
            InstanceType::Resource => "Resources",
            InstanceType::State => "States",
        }
    }

    pub fn param_name(&self) -> &str {
        match self {
            InstanceType::Emitter => "emitters",
            InstanceType::Resource => "resources",
            InstanceType::State => "states",
        }
    }
}

fn is_type_with_name(ty: &Type, name: &str) -> bool {
    match ty {
        Type::Path(p) => p.path.is_ident(name),
        _ => false,
    }
}

struct InstanceTracker<'l> {
    param_name: Option<&'l Ident>,
    acquire_as: Option<&'l Ident>,
    manager_call: Option<SystemParam<'l>>,
    mut_calls: BTreeMap<&'l Ident, SystemParam<'l>>,
    raw_calls: BTreeMap<&'l Ident, SystemParam<'l>>,
    ref_calls: BTreeMap<&'l Ident, SystemParam<'l>>,
    type_of: InstanceType,
}

impl<'l> InstanceTracker<'l> {
    pub fn new(type_of: InstanceType) -> Self {
        Self {
            param_name: None,
            acquire_as: None,
            manager_call: None,
            mut_calls: BTreeMap::new(),
            raw_calls: BTreeMap::new(),
            ref_calls: BTreeMap::new(),
            type_of,
        }
    }

    pub fn set_param(&mut self, name: &'l Ident) {
        self.param_name = Some(name);
    }

    // pub fn set_manager(&mut self, name: &'l Ident, param: SystemParam<'l>) {
    //     if self.manager_call.is_none() {
    //         self.acquire_as = Some(name);
    //         self.manager_call = Some(param);
    //     } else {
    //         let manager_name = self.type_of.manager_name();

    //         panic!(
    //             "Cannot use multiple managers or a mutable and immutable manager together. Use {}Mut or {}Ref distinctly.",
    //             manager_name,
    //             manager_name,
    //         );
    //     }
    // }

    pub fn add_call(&mut self, name: &'l Ident, param: SystemParam<'l>) {
        if self.manager_call.is_some() {
            let manager_name = self.type_of.manager_name();

            panic!(
                "Cannot access values from a manager when also accessing the manager itself. Found {}Mut or {}Ref.",
                manager_name,
                manager_name,
            );
        }

        match param {
            SystemParam::ParamMut(_) => {
                self.mut_calls.insert(name, param);
            }
            SystemParam::ParamRaw(_) => {
                self.raw_calls.insert(name, param);
            }
            SystemParam::ParamRef(_) => {
                self.ref_calls.insert(name, param);
            }
            SystemParam::ParamRefWithExtract(_, _) => {
                self.ref_calls.insert(name, param);
            }
            SystemParam::ArgsRef(_) => {
                self.ref_calls.insert(name, param);
            } // _ => unimplemented!(),
        };

        if self.mut_calls.len() > 1 {
            panic!(
                "Only 1 mutable {} parameter is allowed per system function. If you need multiple, try using raw parameters.",
                self.type_of.param_name(),
            );
        }

        if !self.ref_calls.is_empty() && !self.mut_calls.is_empty() {
            panic!(
                "Cannot mix mutable and immutable {} parameters in the same system function.",
                self.type_of.param_name(),
            );
        }
    }

    pub fn generate_param_name(&self) -> Ident {
        self.param_name
            .map(|n| n.to_owned())
            .unwrap_or_else(|| format_ident!("{}", self.type_of.param_name()))
    }

    pub fn generate_quotes(self) -> Vec<proc_macro2::TokenStream> {
        let mut quotes = vec![];

        if self.manager_call.is_none()
            && self.mut_calls.is_empty()
            && self.raw_calls.is_empty()
            && self.ref_calls.is_empty()
        {
            return quotes;
        }

        let manager_param_name = self.generate_param_name();
        let manager_var_name = self
            .acquire_as
            .map(|n| n.to_owned())
            .unwrap_or_else(|| manager_param_name.clone());

        // Read/write lock acquires for the manager
        // let manager_call = self.manager_call.unwrap_or(if self.mut_calls.is_empty() {
        //     SystemParam::ManagerRef
        // } else {
        //     SystemParam::ManagerMut
        // });

        // match manager_call {
        //     SystemParam::ManagerMut => {
        //         quotes.push(quote! {
        //             let mut #manager_var_name = #manager_param_name.write().await;
        //         });
        //     }
        //     SystemParam::ManagerRef => {
        //         quotes.push(quote! {
        //             let #manager_var_name = #manager_param_name.read().await;
        //         });
        //     }
        //     _ => unimplemented!(),
        // };

        // Get/set calls on the manager
        let is_emitter = matches!(self.type_of, InstanceType::Emitter);
        let mut calls = vec![];
        calls.extend(&self.mut_calls);
        calls.extend(&self.raw_calls);
        calls.extend(&self.ref_calls);

        let mut use_state_import = false;

        for (name, param) in calls {
            let base_name = format_ident!("{}_base", name);

            match param {
                SystemParam::ParamMut(ty) => {
                    if is_emitter {
                        quotes.push(quote! {
                            let mut #base_name = #manager_var_name.get::<starbase::Emitter<#ty>>().await;
                            let #name = #base_name.write();
                        });
                    } else {
                        quotes.push(quote! {
                            let mut #base_name = #manager_var_name.get::<#ty>().await;
                            let #name = #base_name.write();
                        });
                    }
                }
                SystemParam::ParamRaw(ty) => {
                    if is_emitter {
                        quotes.push(quote! {
                            let mut #name = #manager_var_name.get::<starbase::Emitter<#ty>>().await;
                        });
                    } else {
                        quotes.push(quote! {
                            let mut #name = #manager_var_name.get::<#ty>().await;
                        });
                    }
                }
                SystemParam::ParamRef(ty) => {
                    if is_emitter {
                        quotes.push(quote! {
                            let #base_name = #manager_var_name.get::<starbase::Emitter<#ty>>().await;
                            let #name = #base_name.read();
                        });
                    } else {
                        quotes.push(quote! {
                            let #base_name = #manager_var_name.get::<#ty>().await;
                            let #name = #base_name.read();
                        });
                    }
                }
                SystemParam::ParamRefWithExtract(ty, extract_ty) => {
                    quotes.push(quote! {
                        let #name = {
                            #manager_var_name.get::<#ty>().await.read().extract::<#extract_ty>()
                        };
                    });
                }
                SystemParam::ArgsRef(ty) => {
                    if !use_state_import {
                        quotes.push(quote! { use starbase::StateInstance; });
                        use_state_import = true;
                    }

                    quotes.push(quote! {
                        let #name = {
                            #manager_var_name.get::<starbase::ExecuteArgs>().await.read().extract::<#ty>().unwrap()
                        };
                    });
                } // _ => unimplemented!(),
            };
        }

        quotes
    }
}

fn default_true() -> bool {
    true
}

#[derive(Debug, FromMeta)]
struct SystemArgs {
    #[darling(default = "default_true")]
    instrument: bool,
}

// #[system]
pub fn macro_impl(base_args: TokenStream, item: TokenStream) -> TokenStream {
    let attr_args = NestedMeta::parse_meta_list(base_args.into()).unwrap();
    let args = SystemArgs::from_list(&attr_args).unwrap();
    let func = parse_macro_input!(item as syn::ItemFn);
    let func_name = func.sig.ident;
    let func_body = func.block;
    let func_vis = func.vis;

    // Types of instances
    let mut states = InstanceTracker::new(InstanceType::State);
    let mut resources = InstanceTracker::new(InstanceType::Resource);
    let mut emitters = InstanceTracker::new(InstanceType::Emitter);

    // Convert inputs to system param enums
    for i in &func.sig.inputs {
        let FnArg::Typed(input) = i else {
            panic!("&self not permitted in system functions.");
        };

        let var_name = match input.pat.as_ref() {
            Pat::Ident(ref pat) => &pat.ident,
            _ => panic!("Unsupported parameter identifier pattern."),
        };

        match input.ty.as_ref() {
            Type::Path(ref path) => {
                // TypeWrapper<InnerType>
                let segment = path
                    .path
                    .segments
                    .first()
                    .unwrap_or_else(|| panic!("Required a parameter type for {}.", var_name));

                // TypeWrapper
                let type_wrapper = segment.ident.to_string();

                if segment.arguments.is_empty() {
                    match type_wrapper.as_ref() {
                        "Emitters" => {
                            emitters.set_param(var_name);
                        }
                        "Resources" => {
                            resources.set_param(var_name);
                        }
                        "States" => {
                            states.set_param(var_name);
                        }
                        wrapper => {
                            panic!("Unknown parameter type {} for {}.", wrapper, var_name);
                        }
                    };
                } else {
                    // <InnerType>
                    let PathArguments::AngleBracketed(segment_args) = &segment.arguments else {
                        panic!("Required a generic parameter type for {}.", type_wrapper);
                    };

                    let mut segment_iter = segment_args.args.iter();

                    // InnerType
                    let GenericArgument::Type(inner_type) = segment_iter.next().unwrap() else {
                        panic!("Required a generic parameter type for {}.", type_wrapper);
                    };

                    match type_wrapper.as_ref() {
                        "EmitterMut" => {
                            emitters.add_call(var_name, SystemParam::ParamMut(inner_type));
                        }
                        "EmitterRaw" => {
                            emitters.add_call(var_name, SystemParam::ParamRaw(inner_type));
                        }
                        "EmitterRef" => {
                            emitters.add_call(var_name, SystemParam::ParamRef(inner_type));
                        }
                        "ResourceMut" => {
                            resources.add_call(var_name, SystemParam::ParamMut(inner_type));
                        }
                        "ResourceRaw" => {
                            resources.add_call(var_name, SystemParam::ParamRaw(inner_type));
                        }
                        "ResourceRef" => {
                            resources.add_call(var_name, SystemParam::ParamRef(inner_type));
                        }
                        "StateMut" => {
                            states.add_call(var_name, SystemParam::ParamMut(inner_type));
                        }
                        "StateRaw" => {
                            states.add_call(var_name, SystemParam::ParamRaw(inner_type));
                        }
                        "StateRef" => {
                            if let Some(GenericArgument::Type(extract_type)) = segment_iter.next() {
                                if is_type_with_name(inner_type, "ExecuteArgs") {
                                    states.add_call(var_name, SystemParam::ArgsRef(extract_type));
                                } else {
                                    states.add_call(
                                        var_name,
                                        SystemParam::ParamRefWithExtract(inner_type, extract_type),
                                    );
                                }
                            } else {
                                states.add_call(var_name, SystemParam::ParamRef(inner_type));
                            }
                        }
                        "ArgsRef" => {
                            states.add_call(var_name, SystemParam::ArgsRef(inner_type));
                        }
                        wrapper => {
                            panic!("Unknown parameter type {} for {}.", wrapper, var_name);
                        }
                    };
                }
            }
            _ => panic!("Unsupported parameter type for {}.", var_name),
        };
    }

    let state_param = states.generate_param_name();
    let state_quotes = states.generate_quotes();
    let resource_param = resources.generate_param_name();
    let resource_quotes = resources.generate_quotes();
    let emitter_param = emitters.generate_param_name();
    let emitter_quotes = emitters.generate_quotes();

    let attributes = if cfg!(feature = "tracing") && args.instrument {
        quote! {
            #[tracing::instrument(skip_all)]
        }
    } else {
        quote! {}
    };

    quote! {
        #attributes
        #func_vis async fn #func_name(
            #state_param: starbase::States,
            #resource_param: starbase::Resources,
            #emitter_param: starbase::Emitters
        ) -> starbase::SystemResult {
            #(#state_quotes)*
            #(#resource_quotes)*
            #(#emitter_quotes)*
            #func_body
            Ok(())
        }
    }
    .into()
}
