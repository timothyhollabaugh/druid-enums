use proc_macro2::TokenStream;
use quote::{format_ident, quote};
use syn::{parse_macro_input, Fields};

mod parse;
use parse::{MatcherDerive, MatcherVariant};

#[proc_macro_derive(Matcher, attributes(matcher))]
pub fn derive(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    // TODO when we generate a name that isn't a valid ident or is a keyword, generate a different
    // name rather than panicking.
    // TODO handle generics in the input
    let input = parse_macro_input!(input as MatcherDerive);

    let visibility = &input.visibility;
    let enum_name = &input.enum_name;
    let matcher_name = input.resolve_matcher_name();

    // Returns the `T` in `Widget<T>` for the variant.
    fn type_of(variant: &MatcherVariant) -> TokenStream {
        match &variant.fields {
            Fields::Unit => quote!(()),
            Fields::Unnamed(fields) if fields.unnamed.is_empty() => quote!(()),
            Fields::Unnamed(fields) => {
                let types = fields.unnamed.iter().map(|f| &f.ty);
                quote!((#(#types),*))
            }
            Fields::Named(_) => unreachable!(),
        }
    }

    // Returns (pattern to match for, `data` param for the widget).
    fn data_of(variant: &MatcherVariant, prefix: &str) -> (TokenStream, TokenStream) {
        match &variant.fields {
            Fields::Unit => (quote!(), quote!(&mut ())),
            Fields::Unnamed(fields) if fields.unnamed.is_empty() => (quote!(()), quote!(&mut ())),
            Fields::Unnamed(fields) => {
                let names: Vec<syn::Ident> = fields
                    .unnamed
                    .iter()
                    .enumerate()
                    .map(|(i, _)| format_ident!("{}p{}", prefix, i))
                    .collect();
                (quote!((#(#names),*)), quote!((#(#names),*)))
            }
            Fields::Named(_) => unreachable!(),
        }
    }

    let struct_fields = input.variants.iter().map(|variant| {
        let builder_name = variant.resolve_builder_name();
        let variant_ty = type_of(&variant);
        quote!(#builder_name: Option<::druid::WidgetPod<(Shared, #variant_ty), Box<dyn ::druid::Widget<(Shared, #variant_ty)>>>>)
    });

    let struct_defaults = input.variants.iter().map(|variant| {
        let builder_name = variant.resolve_builder_name();
        quote!(#builder_name: None)
    });

    let builder_fns = input.variants.iter().map(|variant| {
        let builder_name = variant.resolve_builder_name();
        let variant_ty = type_of(&variant);
        quote! {
            pub fn #builder_name(mut self, widget: impl ::druid::Widget<(Shared, #variant_ty)> + 'static) -> Self {
                self.#builder_name = Some(::druid::WidgetPod::new(Box::new(widget)));
                self
            }
        }
    });

    let widget_added_checks = input.variants.iter().map(|variant| {
        let builder_name = variant.resolve_builder_name();
        quote! {
            if self.default_.is_none() && self.#builder_name.is_none() {
                ::log::warn!("{}::{} variant of {:?} has not been set.", stringify!(#matcher_name), stringify!(#builder_name), ctx.widget_id());
            }
        }
    });

    let event_match = input.variants.iter().map(|variant| {
        let builder_name = variant.resolve_builder_name();
        let variant_name = &variant.name;
        let (data_pattern, data_values) = data_of(&variant, "");
        quote! {
            #enum_name::#variant_name #data_pattern => match &mut self.#builder_name {
                Some(widget) => {
                    let mut d = (data.0.to_owned(), #data_values.to_owned());
                    widget.event(ctx, event, &mut d, env);
                    *data = (
                        d.0,
                        #enum_name::#variant_name(d.1),
                    );
                },
                None => (),
            }
        }
    });

    let lifecycle_match = input.variants.iter().map(|variant| {
        let builder_name = variant.resolve_builder_name();
        let variant_name = &variant.name;
        let (data_pattern, data_values) = data_of(&variant, "");
        quote! {
            #enum_name::#variant_name #data_pattern => match &mut self.#builder_name {
                Some(widget) => widget.lifecycle(ctx, event, &(data.0.to_owned(), #data_values.to_owned()), env),
                None => (),
            }
        }
    });

    let update_match = input.variants.iter().map(|variant| {
        let builder_name = variant.resolve_builder_name();
        let variant_name = &variant.name;
        let (old_data_pattern, _old_data_values) = data_of(&variant, "old_");
        let (data_pattern, data_values) = data_of(&variant, "");
        quote! {
            (#enum_name::#variant_name #old_data_pattern, #enum_name::#variant_name #data_pattern) => {
                match &mut self.#builder_name {
                    Some(widget) => widget.update(ctx, &(data.0.to_owned(), #data_values.to_owned()), env),
                    None => (),
                }
            }
        }
    });

    let layout_match = input.variants.iter().map(|variant| {
        let builder_name = variant.resolve_builder_name();
        let variant_name = &variant.name;
        let (data_pattern, data_values) = data_of(&variant, "");
        quote! {
            #enum_name::#variant_name #data_pattern => match &mut self.#builder_name {
                Some(widget) => {
                    let size = widget.layout(ctx, bc, &(data.0.to_owned(), #data_values.to_owned()), env);
                    widget.set_layout_rect(ctx, &(data.0.to_owned(), #data_values.to_owned()), env, size.to_rect());
                    size
                },
                None => bc.min(),
            }
        }
    });

    let paint_match = input.variants.iter().map(|variant| {
        let builder_name = variant.resolve_builder_name();
        let variant_name = &variant.name;
        let (data_pattern, data_values) = data_of(&variant, "");
        quote! {
            #enum_name::#variant_name #data_pattern => match &mut self.#builder_name {
                Some(widget) => widget.paint(ctx, &(data.0.to_owned(), #data_values.to_owned()), env),
                None => (),
            }
        }
    });

    let output = quote! {
        impl #enum_name {
            pub fn matcher<Shared: ::druid::Data>() -> #matcher_name<Shared> {
                #matcher_name::new()
            }
        }

        #visibility struct #matcher_name<Shared: ::druid::Data> {
            #(#struct_fields,)*
            default_: Option<Box<dyn ::druid::Widget<#enum_name>>>,
            discriminant_: Option<::std::mem::Discriminant<#enum_name>>,
        }

        impl<Shared> #matcher_name<Shared> where Shared: ::druid::Data {
            pub fn new() -> Self {
                Self {
                    #(#struct_defaults,)*
                    default_: None,
                    discriminant_: None,
                }
            }
            pub fn default(mut self, widget: impl ::druid::Widget<#enum_name> + 'static) -> Self {
                self.default_ = Some(Box::new(widget));
                self
            }
            pub fn default_empty(mut self) -> Self {
                self.default_ = Some(Box::new(::druid::widget::SizedBox::empty()));
                self
            }
            #(#builder_fns)*
        }

        impl<Shared> ::druid::Widget<(Shared, #enum_name)> for #matcher_name<Shared> where Shared: ::druid::Data {
            fn event(
                &mut self,
                ctx: &mut ::druid::EventCtx,
                event: &::druid::Event,
                data: &mut (Shared, #enum_name),
                env: &::druid::Env
            ) {
                if self.discriminant_ == Some(::std::mem::discriminant(&data.1)) {
                    match &mut data.1 {
                        #(#event_match)*
                    }
                }
            }
            fn lifecycle(
                &mut self,
                ctx: &mut ::druid::LifeCycleCtx,
                event: &::druid::LifeCycle,
                data: &(Shared, #enum_name),
                env: &::druid::Env
            ) {
                self.discriminant_ = Some(::std::mem::discriminant(&data.1));
                if let ::druid::LifeCycle::WidgetAdded = event {
                    #(#widget_added_checks)*
                }
                match &data.1 {
                    #(#lifecycle_match)*
                }
            }
            fn update(&mut self,
                ctx: &mut ::druid::UpdateCtx,
                old_data: &(Shared, #enum_name),
                data: &(Shared, #enum_name),
                env: &::druid::Env
            ) {
                match (&old_data.1, &data.1) {
                    #(#update_match)*
                    _ => {
                        ctx.children_changed();
                    }
                }
            }
            fn layout(
                &mut self,
                ctx: &mut ::druid::LayoutCtx,
                bc: &::druid::BoxConstraints,
                data: &(Shared, #enum_name),
                env: &::druid::Env
            ) -> ::druid::Size {
                match &data.1 {
                    #(#layout_match)*
                }
            }
            fn paint(&mut self, ctx: &mut ::druid::PaintCtx, data: &(Shared, #enum_name), env: &::druid::Env) {
                match &data.1 {
                    #(#paint_match)*
                }
            }
        }
    };
    output.into()
}
