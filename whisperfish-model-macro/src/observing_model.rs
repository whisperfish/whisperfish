use super::parse_until;
use quote::quote;
use syn::punctuated::Punctuated;

pub struct ObservingModelAttribute {
    segments: Punctuated<syn::Meta, syn::Token!(,)>,
}

#[derive(Debug)]
struct RoleProperties {
    field: syn::Ident,
    role_type: syn::TypePath,
    role_signal: syn::Ident,
    optional: bool,
    // Name -> Role mapping
    properties: Vec<(syn::Ident, String)>,
}

impl syn::parse::Parse for RoleProperties {
    fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
        // Structure : `field: Type {name Name, name Name, ...}`
        let field = input.parse::<syn::Ident>()?;
        let _ = input.parse::<syn::Token!(:)>()?;
        let ty = input.parse::<syn::TypePath>()?;
        assert!(ty.qself.is_none());

        let optional = ty.path.segments.first().unwrap().ident == "Option";
        let role_type = if optional {
            match &ty.path.segments.first().unwrap().arguments {
                syn::PathArguments::AngleBracketed(args) => {
                    let ty = args.args.first().unwrap();
                    if let syn::GenericArgument::Type(ty) = ty {
                        if let syn::Type::Path(ty) = ty {
                            ty.clone()
                        } else {
                            panic!("expected a type path");
                        }
                    } else {
                        panic!("expected a type");
                    }
                }
                _ => panic!("expected angle bracketed arguments for Option<T>"),
            }
        } else {
            ty
        };

        let _notify = input.parse::<syn::Ident>()?;
        assert!(_notify == "NOTIFY");
        let role_signal = input.parse::<syn::Ident>()?;

        let content;
        let _brace_token = syn::braced!(content in input);

        let mut properties = Vec::new();

        while content.peek(syn::Ident) {
            let name = content.parse::<syn::Ident>()?;
            let role = content.parse::<syn::Ident>()?;

            properties.push((name, role.to_string()));

            if content.parse::<syn::Token!(,)>().is_err() {
                // Allow trailing comma
                break;
            }
        }

        Ok(Self {
            field,
            role_signal,
            role_type,
            optional,
            properties,
        })
    }
}

impl ObservingModelAttribute {
    fn get(&self, _attr_name: &str) -> Option<&proc_macro2::TokenStream> {
        self.segments
            .iter()
            .map(|x| {
                if let syn::Meta::List(val) = x {
                    val
                } else {
                    panic!("Could not parse {x:?}");
                }
            })
            .filter(|f| f.path.get_ident().unwrap() == _attr_name)
            .map(|x| &x.tokens)
            .next()
    }

    fn properties_from_role(&self) -> Option<RoleProperties> {
        let meta = self.get("properties_from_role")?;
        Some(
            syn::parse2::<RoleProperties>(meta.clone())
                .unwrap_or_else(|_| panic!("parse properties_from_role {meta:?}")),
        )
    }
}

impl syn::parse::Parse for ObservingModelAttribute {
    fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
        let mut segments = Punctuated::new();

        while !input.is_empty() {
            let next = parse_until(input, syn::Token!(,))?;
            segments.push_value(syn::parse2(next)?);
            match input.parse() {
                Ok(punct) => {
                    segments.push_punct(punct);
                }
                Err(_) => break,
            }
        }

        Ok(Self { segments })
    }
}

fn extract_field_attr(field: &mut syn::Field, property_name: &str) -> Option<syn::Attribute> {
    let idx = field.attrs.iter().position(|attr| {
        attr.path()
            .get_ident()
            .map(syn::Ident::to_string)
            .as_deref()
            == Some(property_name)
    })?;
    Some(field.attrs.remove(idx))
}

#[derive(Debug)]
struct QtProperty {
    read: Option<syn::Ident>,
    write: Option<syn::Ident>,
    alias: Option<syn::Ident>,
    notify: Option<syn::Ident>,
}

impl syn::parse::Parse for QtProperty {
    fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
        let mut read = None;
        let mut write = None;
        let mut alias = None;
        let mut notify = None;

        while !input.is_empty() {
            let ident = input.parse::<syn::Ident>()?;
            let _ = input.parse::<syn::Token!(:)>()?;
            match ident.to_string().as_str() {
                "READ" => {
                    read = Some(input.parse::<syn::Ident>()?);
                }
                "WRITE" => {
                    write = Some(input.parse::<syn::Ident>()?);
                }
                "ALIAS" => {
                    alias = Some(input.parse::<syn::Ident>()?);
                }
                "NOTIFY" => {
                    notify = Some(input.parse::<syn::Ident>()?);
                }
                _ => panic!("unexpected token {:?}", ident),
            }
            if input.parse::<syn::Token!(,)>().is_err() {
                break;
            }
        }

        Ok(Self {
            read,
            write,
            alias,
            notify,
        })
    }
}

impl QtProperty {
    fn attrs(&self) -> impl Iterator<Item = proc_macro2::TokenStream> + '_ {
        [
            ("READ", self.read.as_ref()),
            ("WRITE", self.write.as_ref()),
            ("ALIAS", self.alias.as_ref()),
            ("NOTIFY", self.notify.as_ref()),
        ]
        .into_iter()
        .filter_map(|(name, ident)| {
            let name_ident = syn::Ident::new(name, proc_macro2::Span::call_site());
            let ident = ident?;
            let ident_str = ident.to_string();
            let ident = match name {
                "ALIAS" | "NOTIFY" => ident.clone(),
                "READ" => syn::Ident::new(&format!("_{}", ident_str), ident.span()),
                "WRITE" => syn::Ident::new(&format!("_{}", ident_str), ident.span()),
                _ => unreachable!("unexpected token {}", name),
            };
            Some(quote! {#name_ident #ident})
        })
    }

    fn methods<'a>(
        &'a self,
        ty: &'a syn::Type,
    ) -> impl Iterator<Item = proc_macro2::TokenStream> + 'a {
        let ctx = quote! {
            let storage = self._app.as_pinned().and_then(|app| app.borrow().storage.borrow().clone());
            let addr = self._observing_model_registration.as_ref().map(|omr| omr.actor.clone());
            let ctx = storage.clone().zip(addr).map(|(storage, addr)| {
                crate::model::active_model::ModelContext {
                    storage,
                    addr,
                }
            });
        };

        [("READ", self.read.as_ref()), ("WRITE", self.write.as_ref())]
            .into_iter()
            .filter_map(move |(name, ident)| {
                let ident = ident?;
                let ident_str = ident.to_string();
                Some(match name {
                    "READ" => {
                        let wrapping_ident =
                            syn::Ident::new(&format!("_{}", ident_str), ident.span());
                        quote! {
                            #[qmeta_async::with_executor]
                            fn #wrapping_ident(&self) -> #ty {
                                #ctx
                                self.#ident(ctx)
                            }
                        }
                    }
                    "WRITE" => {
                        let wrapping_ident =
                            syn::Ident::new(&format!("_{}", ident_str), ident.span());
                        quote! {
                            #[qmeta_async::with_executor]
                            fn #wrapping_ident(&mut self, val: #ty) {
                                #ctx
                                self.#ident(ctx, val)
                            }
                        }
                    }
                    _ => unreachable!("unexpected token {}", name),
                })
            })
    }
}

pub fn inject_literal_properties(strukt: &mut syn::ItemStruct) -> proc_macro2::TokenStream {
    let mut methods = Vec::<proc_macro2::TokenStream>::new();

    for field in &mut strukt.fields {
        let Some(attr) = extract_field_attr(field, "qt_property") else {
            continue;
        };
        let syn::Meta::List(list) = attr.meta else {
            panic!(
                "Parse error for {} attribute {:?}",
                field.ident.as_ref().unwrap(),
                attr.meta
            );
        };

        let property = syn::parse2::<QtProperty>(list.tokens).expect("expected a property name");
        let name = field.ident.as_ref().unwrap();
        let ty = &field.ty;

        let attrs = property.attrs();

        methods.extend(property.methods(ty));

        *field = syn::parse_quote! { #name: qt_property!(#ty; #(#attrs)*) };
    }

    let ty = &strukt.ident;
    quote! {
        impl #ty {
            #(#methods)*
        }
    }
}

pub fn inject_struct_fields(attr: &ObservingModelAttribute, strukt: &mut syn::FieldsNamed) {
    inject_base_fields(strukt);
    inject_role_fields(attr, strukt);
}

fn inject_base_fields(fields: &mut syn::FieldsNamed) {
    fields
        .named
        .extend::<Vec<syn::Field>>(vec![
            syn::parse_quote! { _app: qt_property!(QPointer<crate::gui::AppState>; ALIAS app WRITE set_app) },
            syn::parse_quote! { _observing_model_registration: Option<crate::model::ObservingModelRegistration<Self>> },
        ]);
}

fn inject_role_fields(attr: &ObservingModelAttribute, fields: &mut syn::FieldsNamed) {
    let Some(role) = attr.properties_from_role() else {
        return;
    };

    if role.properties.is_empty() {
        return;
    }

    let role_signal = &role.role_signal;

    for (property, _) in &role.properties {
        let property_str = property.to_string();
        let getter = syn::Ident::new(
            &format!(
                "_get_role_{}",
                property_str.strip_prefix("r#").unwrap_or(&property_str)
            ),
            property.span(),
        );

        fields.named.push(syn::parse_quote! {
            #property : qmetaobject::qt_property!(QVariant; READ #getter NOTIFY #role_signal)
        });
    }
}

pub fn generate_methods(
    attr: &ObservingModelAttribute,
    strukt: &syn::ItemStruct,
) -> proc_macro2::TokenStream {
    let name = &strukt.ident;

    let mut property_getters = Vec::new();
    if let Some(properties) = attr.properties_from_role() {
        let field = &properties.field;
        let role_type = &properties.role_type;

        for (property, role_variant) in &properties.properties {
            let role_variant = syn::Ident::new(role_variant, proc_macro2::Span::call_site());
            let property_str = property.to_string();
            let getter = syn::Ident::new(
                &format!(
                    "_get_role_{}",
                    property_str.strip_prefix("r#").unwrap_or(&property_str)
                ),
                property.span(),
            );

            property_getters.push(if properties.optional {
                quote! {
                    fn #getter(&self) -> QVariant {
                        match self.#field.as_ref() {
                            Some(x) => {
                                (#role_type::#role_variant).get(x)
                            }
                            None => qmetaobject::QVariant::default()
                        }
                    }
                }
            } else {
                quote! {
                    fn #getter(&self) -> QVariant {
                        (#role_type::#role_variant).get(self.#field.as_ref())
                    }
                }
            });
        }
    }

    quote! {
        impl #name {
            #(
                #property_getters
            )*


            #[qmeta_async::with_executor]
            #[tracing::instrument(skip(self, app))]
            fn set_app(&mut self, app: QPointer<crate::gui::AppState>) {
                self._app = app.clone();
                let this = qmetaobject::QPointer::from(&*self);
                let app = app.as_pinned().expect("Valid AppState initialization");
                let app = app.borrow();
                drop(self);
                app.deferred_with_storage(move |storage| {
                    let Some(this) = this.as_pinned() else {
                        tracing::warn!("Object destroyed before being initialized");
                        return;
                    };
                    this.borrow_mut().reinit(storage);
                });
            }

            fn reinit(&mut self, mut storage: crate::store::Storage) {
                use actix::prelude::*;

                let actor = ObservingModelActor {
                    model: qmetaobject::QPointer::from(&*self),
                    storage: storage.clone(),
                }
                .start();

                let ctx = crate::model::active_model::ModelContext {
                    storage: storage.clone(),
                    addr: actor.clone(),
                };
                self.init(ctx);

                let handle = storage.register_observer(
                    crate::store::observer::EventObserving::interests(self),
                    actor.downgrade().recipient(),
                );

                self._observing_model_registration = Some(ObservingModelRegistration {
                    actor,
                    observer_handle: handle,
                });
            }

            fn storage(&self) -> crate::store::Storage {
                self._app.as_pinned()
                    .expect("app set by QML")
                    .borrow()
                    .storage
                    .borrow()
                    .clone()
                    .expect("app initialized with storage")
            }

            fn update_interests(&self) {
                let mut storage = self.storage();
                if let Some(omr) = self._observing_model_registration.as_ref() {
                    storage.update_interests(omr.observer_handle, self.interests());
                }
            }
        }
    }
}
