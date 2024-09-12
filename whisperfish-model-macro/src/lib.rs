extern crate proc_macro;
use proc_macro::TokenStream;
use quote::quote;
use syn::punctuated::Punctuated;

struct ObservingModelAttribute {
    segments: Punctuated<syn::Meta, syn::Token!(,)>,
}

#[derive(Debug)]
struct RoleProperties {
    field: syn::Ident,
    role_type: syn::TypePath,
    optional: bool,
    // Name -> Role mapping
    properties: Vec<(String, String)>,
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

        let content;
        let _brace_token = syn::braced!(content in input);

        let mut properties = Vec::new();

        while content.peek(syn::Ident) {
            let name = content.parse::<syn::Ident>()?;
            let role = content.parse::<syn::Ident>()?;

            properties.push((name.to_string(), role.to_string()));

            if content.parse::<syn::Token!(,)>().is_err() {
                // Allow trailing comma
                break;
            }
        }

        Ok(Self {
            field,
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

fn parse_until<E: syn::parse::Peek>(
    input: syn::parse::ParseStream,
    end: E,
) -> syn::Result<proc_macro2::TokenStream> {
    let mut tokens = proc_macro2::TokenStream::new();
    while !input.is_empty() && !input.peek(end) {
        let next: proc_macro2::TokenTree = input.parse()?;
        tokens.extend(Some(next));
    }
    Ok(tokens)
}

impl syn::parse::Parse for ObservingModelAttribute {
    fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
        // XXX Currently, this doesn't parse trailing commas
        let mut segments = Punctuated::new();

        let first = parse_until(input, syn::Token!(,))?;
        segments.push_value(syn::parse2(first)?);

        while input.peek(syn::Token!(,)) {
            segments.push_punct(input.parse()?);

            let next = parse_until(input, syn::Token!(,))?;
            segments.push_value(syn::parse2(next)?);
        }

        Ok(Self { segments })
    }
}

#[proc_macro_attribute]
pub fn observing_model(attr: TokenStream, input: TokenStream) -> TokenStream {
    let mut strukt = syn::parse_macro_input!(input as syn::ItemStruct);
    let attr = syn::parse_macro_input!(attr as ObservingModelAttribute);

    let syn::Fields::Named(fields) = &mut strukt.fields else {
        return TokenStream::from(quote! {
            compile_error!("expected a struct with named fields");
        });
    };

    inject_struct_fields(&attr, fields);
    let methods = generate_methods(&attr, &strukt);

    TokenStream::from(quote! {
        #strukt
        #methods
    })
}

fn inject_struct_fields(attr: &ObservingModelAttribute, strukt: &mut syn::FieldsNamed) {
    inject_base_fields(strukt);
    inject_role_fields(attr, strukt);
}

fn inject_base_fields(fields: &mut syn::FieldsNamed) {
    fields
        .named
        .extend::<Vec<syn::Field>>(vec![
            syn::parse_quote! { _app: QPointer<crate::gui::AppState> },
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

    fields.named.push(syn::parse_quote! {
        _role_property_changed: qmetaobject::qt_signal!()
    });

    for (property, _) in &role.properties {
        let getter = syn::Ident::new(
            &format!("_get_role_{}", property),
            proc_macro2::Span::call_site(),
        );
        let property = syn::Ident::new(property, proc_macro2::Span::call_site());

        fields.named.push(syn::parse_quote! {
            #property : qmetaobject::qt_property!(QVariant; READ #getter NOTIFY _role_property_changed)
        });
    }
}

fn generate_methods(
    attr: &ObservingModelAttribute,
    strukt: &syn::ItemStruct,
) -> proc_macro2::TokenStream {
    let name = &strukt.ident;

    let mut property_getters = Vec::new();
    if let Some(properties) = attr.properties_from_role() {
        let field = &properties.field;
        let role_type = &properties.role_type;

        for (property, role_variant) in &properties.properties {
            let property = syn::Ident::new(property, proc_macro2::Span::call_site());
            let role_variant = syn::Ident::new(role_variant, proc_macro2::Span::call_site());
            let getter = syn::Ident::new(
                &format!("_get_role_{}", property),
                proc_macro2::Span::call_site(),
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
                self._app = app;
                self.reinit();
            }

            fn reinit(&mut self) {
                use actix::prelude::*;
                let ptr = qmetaobject::QPointer::from(&*self);
                if let Some(app) = self._app.as_pinned() {
                    let storage = app.borrow().storage.borrow().clone();
                    if let Some(mut storage) = storage {
                        let actor = ObservingModelActor {
                            model: ptr,
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
                }
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
