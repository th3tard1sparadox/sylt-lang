use lazy_static::lazy_static;
use quote::{format_ident, quote};
use std::collections::HashMap;
use std::path::Path;
use std::str::FromStr;
use std::sync::{Arc, Mutex};
use syn::{
    parse::{Parse, ParseStream, Result},
    parse_macro_input, Expr, Pat, Token,
};

struct ExternBlock {
    pattern: Pat,
    _fat_arrow: Token![=>],
    block: Expr,
}

struct ExternFunction {
    module: syn::LitStr,
    function: syn::Ident,
    name: Option<syn::Ident>,
    doc: syn::LitStr,
    signature: syn::LitStr,
    blocks: Vec<ExternBlock>,
}

impl Parse for ExternBlock {
    fn parse(input: ParseStream) -> Result<Self> {
        Ok(Self {
            pattern: input.parse()?,
            _fat_arrow: input.parse()?,
            block: input.parse()?,
        })
    }
}

impl Parse for ExternFunction {
    fn parse(input: ParseStream) -> Result<Self> {
        let module = input.parse()?;
        let _: Token![,] = input.parse()?;
        let function = input.parse()?;
        let name = if input.peek(Token![as]) {
            let _as: Token![as] = input.parse()?;
            input.parse()?
        } else {
            None
        };
        let _: Token![,] = input.parse()?;
        let mut doc = None;
        let mut ty = None;
        loop {
            if input.peek(Token![?]) {
                let _: Token![?] = input.parse()?;
                doc = Some(input.parse()?);
            } else if input.peek(Token![->]) {
                let _: Token![->] = input.parse()?;
                ty = Some(input.parse()?);
            } else {
                break
            }
            let _: Token![,] = input.parse()?;
        }
        if doc.is_none() {
            panic!("Function {} lacks documentation", function);
        }
        if ty.is_none() {
            panic!("Function {} lacks function signature", function);
        }
        let mut blocks = Vec::new();
        while !input.is_empty() {
            blocks.push(input.parse()?);
            let _: Option<Token![,]> = input.parse()?;
        }
        Ok(Self {
            module,
            function,
            name,
            doc: doc.unwrap(),
            signature: ty.unwrap(),
            blocks,
        })
    }
}

#[proc_macro]
pub fn extern_function(tokens: proc_macro::TokenStream) -> proc_macro::TokenStream {
    let parsed: ExternFunction = parse_macro_input!(tokens);
    let module = parsed.module;
    let function = parsed.function;
    let link_name = parsed.name.unwrap_or_else(|| function.clone());
    let doc = parsed.doc;
    let signature = parsed.signature;

    let eval_blocks: Vec<_> = parsed
        .blocks
        .iter()
        .map(|block| {
            let pat = block.pattern.clone();
            let expr = block.block.clone();
            quote! {
                #pat => #expr
            }
        })
        .collect();

    let tokens = quote! {
        #[sylt_macro::sylt_doc(#link_name, #doc, #signature)]
        #[sylt_macro::sylt_link(#link_name, #module, #signature)]
        pub fn #function (
            ctx: ::sylt_common::RuntimeContext
        ) -> ::std::result::Result<::sylt_common::Value, ::sylt_common::error::RuntimeError>
        {
            use ::sylt_common::RustFunction;
            use ::sylt_common::Value::*;
            let values = ctx.machine.stack_from_base(ctx.stack_base);
            match &*values {
                #(#eval_blocks),*
                _ => Err(::sylt_common::error::RuntimeError::ExternTypeMismatch(
                    stringify!(#function).to_string(),
                    values.iter().map(|v| ::sylt_common::Type::from(v)).collect()
                ))
            }
        }
    };
    proc_macro::TokenStream::from(tokens)
}

struct LinkRename {
    _as: Token![as],
    name: syn::Ident,
}

impl Parse for LinkRename {
    fn parse(input: ParseStream) -> Result<Self> {
        Ok(Self {
            _as: input.parse()?,
            name: input.parse()?,
        })
    }
}

struct Link {
    path: syn::Path,
    rename: Option<LinkRename>,
}

impl Parse for Link {
    fn parse(input: ParseStream) -> Result<Self> {
        Ok(Self {
            path: input.parse()?,
            rename: input.parse().ok(),
        })
    }
}

struct Links {
    links: Vec<Link>,
}

impl Parse for Links {
    fn parse(input: ParseStream) -> Result<Self> {
        let mut res = Self { links: Vec::new() };
        while !input.is_empty() {
            res.links.push(input.parse()?);
            let _comma: Option<Token![,]> = input.parse().ok();
        }
        Ok(res)
    }
}

#[proc_macro]
pub fn link(tokens: proc_macro::TokenStream) -> proc_macro::TokenStream {
    let links: Links = parse_macro_input!(tokens);
    let links: Vec<_> = links
        .links
        .iter()
        .map(|link| {
            let name = if let Some(rename) = &link.rename {
                &rename.name
            } else {
                &link.path.segments.last().unwrap().ident
            };
            let path = &link.path;
            quote! {
                (stringify!(#name).to_string(), #path)
            }
        })
        .collect();

    let tokens = quote! {
        vec![ #(#links),* ]
    };
    proc_macro::TokenStream::from(tokens)
}

struct TestSettings {
    errors: String,
    print: bool,
    // Used to tell lua there are runtime errors - since it doesn't care about the type.
    any_runtime_errors: bool,
}

impl Default for TestSettings {
    fn default() -> Self {
        Self {
            errors: String::new(),
            print: true,
            any_runtime_errors: false,
        }
    }
}

fn parse_test_settings(contents: String) -> TestSettings {
    let mut settings = TestSettings::default();

    let mut errors = Vec::new();
    for line in contents.split("\n") {
        if line.starts_with("// error: ") {
            let mut line = line.strip_prefix("// error: ").unwrap().to_string();
            if line.starts_with("$") {
                line = format!(
                    "Error::TypeError {{ kind: TypeError::{}, .. }}",
                    &line[1..]
                );
            }
            if line.starts_with("#") {
                line = format!(
                    "Error::RuntimeError {{ kind: RuntimeError::{}, .. }}",
                    &line[1..]
                );
            }
            if line.starts_with("@") {
                line = format!("Error::SyntaxError {{ span: Span {{ line: {}, ..}}, .. }}", &line[1..]);
            }
            settings.any_runtime_errors |= line.contains("RuntimeError");
            errors.push(line);
        } else if line.starts_with("// flags: ") {
            for flag in line.split(" ").skip(2) {
                match flag {
                    "no_print" => {
                        settings.print = false;
                    }
                    _ => {
                        panic!("Unknown test flag '{}'", flag);
                    }
                }
            }
        }
    }

    settings.errors = format!("[ {} ]", errors.join(", "));
    settings
}

fn find_test_paths(directory: &Path, macro_path: &syn::Path) -> proc_macro2::TokenStream {
    let mut tests = quote! {};

    for entry in std::fs::read_dir(directory).unwrap() {
        let path = entry.unwrap().path();
        let file_name = path.file_name().unwrap().to_str().unwrap();

        if file_name.starts_with("_") {
            continue;
        }

        if path.is_dir() {
            tests.extend(find_test_paths(&path, macro_path));
        } else {
            if !file_name.ends_with(".sy") {
                continue;
            }
            assert!(
                !path.to_str().unwrap().contains(","),
                "You should be ashamed."
            );

            let path_string = path.to_str().unwrap();
            let test_name = format_ident!("{}", file_name.replace(".sy", ""));

            let settings = parse_test_settings(std::fs::read_to_string(path.clone()).unwrap());
            let any_runtime_errors = settings.any_runtime_errors;
            let print = settings.print;
            let wanted_errs: proc_macro2::TokenStream = settings.errors.parse().unwrap();

            // TODO(ed): Make a flag for skipping the test
            let tokens = quote! {
                #macro_path!(#test_name, #path_string, #print, #wanted_errs, #any_runtime_errors);
            };

            tests.extend(tokens);
        }
    }

    let directory = directory
        .file_name()
        .unwrap()
        .to_str()
        .unwrap()
        .replace("/", "");
    let directory = format_ident!("{}", directory);
    quote! {
        mod #directory {
            #tests
        }
    }
}

#[proc_macro]
pub fn find_tests(tokens: proc_macro::TokenStream) -> proc_macro::TokenStream {
    let macro_path: syn::Path = parse_macro_input!(tokens);

    let tokens = find_test_paths(Path::new("tests/"), &macro_path);
    proc_macro::TokenStream::from(tokens)
}

#[proc_macro_derive(Enumerate)]
pub fn derive_enumerate(item: proc_macro::TokenStream) -> proc_macro::TokenStream {
    assert!(!item.is_empty());
    let parsed: syn::ItemEnum = parse_macro_input!(item);

    let ident = parsed.ident;
    let match_from_usize: Vec<_> = parsed
        .variants
        .iter()
        .enumerate()
        .map(|(i, v)| {
            quote! {
                #i => ::std::result::Result::Ok(#ident::#v),
            }
        })
        .collect();
    let match_from_ident: Vec<_> = parsed
        .variants
        .iter()
        .enumerate()
        .map(|(i, v)| {
            quote! {
                #ident::#v => #i,
            }
        })
        .collect();

    let max = parsed.variants.len();

    let item = quote! {
        impl ::std::convert::TryFrom<usize> for #ident {
            type Error = ::std::string::String;

            fn try_from(u: usize) -> ::std::result::Result<Self, Self::Error> {
                match u {
                    #(#match_from_usize)*
                    u => ::std::result::Result::Err(format!("{} only has {} variants, tried {}", stringify!(#ident), #max, u)),
                }
            }
        }

        impl ::std::convert::From<#ident> for usize {
            fn from(s: #ident) -> Self {
                match s {
                    #(#match_from_ident)*
                }
            }
        }
    };
    proc_macro::TokenStream::from(item)
}

#[proc_macro_derive(Next)]
pub fn derive_next(item: proc_macro::TokenStream) -> proc_macro::TokenStream {
    assert!(!item.is_empty());
    let parsed: syn::ItemEnum = parse_macro_input!(item);

    let ident = parsed.ident.clone();

    let mut iter = parsed.variants.iter();
    let mut prev = iter.next().expect("Empty enum can't be Next");
    let mut match_arms: Vec<_> = iter
        .map(|v| {
            let tokens = quote! {
                #ident::#prev => #ident::#v,
            };
            prev = v;
            tokens
        })
        .collect();
    match_arms.push(quote! {
        #ident::#prev => #ident::#prev,
    });

    let item = quote! {
        impl Next for #ident {
            fn next(&self) -> Self {
                match self {
                    #(#match_arms)*
                }
            }
        }
    };
    proc_macro::TokenStream::from(item)
}

#[proc_macro_derive(Numbered)]
pub fn derive_numbered(item: proc_macro::TokenStream) -> proc_macro::TokenStream {
    assert!(!item.is_empty());
    let parsed: syn::ItemEnum = parse_macro_input!(item);

    let ident = parsed.ident.clone();

    let match_arms: Vec<_> = parsed.variants.iter().enumerate().map(|(i, v)| {
        let name = v.ident.clone();
        match v.fields {
            syn::Fields::Named(_) => {
                quote! {
                    #ident::#name { .. } => #i,
                }
            }
            syn::Fields::Unnamed(_) => {
                quote! {
                    #ident::#name ( .. ) => #i,
                }
            }
            syn::Fields::Unit => {
                quote! {
                    #ident::#name => #i,
                }
            }
        }
    }).collect();

    let item = quote! {
        impl Numbered for #ident {
            fn to_number(&self) -> usize {
                match self {
                    #(#match_arms)*
                }
            }
        }
    };
    proc_macro::TokenStream::from(item)
}

enum LinkState {
    Open,
    Written,
}

struct ModuleLink {
    state: LinkState,
    mapping: Vec<(String, String, String)>,
}

impl ModuleLink {
    fn new() -> Self {
        Self {
            state: LinkState::Open,
            mapping: Vec::new(),
        }
    }
}

lazy_static! {
    static ref LINKS: Arc<Mutex<HashMap<String, ModuleLink>>> =
        Arc::new(Mutex::new(HashMap::new()));
}

#[proc_macro]
pub fn sylt_link_gen(tokens: proc_macro::TokenStream) -> proc_macro::TokenStream {
    let module: syn::LitStr = parse_macro_input!(tokens);
    let module = module.value();

    let mut link = LINKS.lock().unwrap();
    let mut link = if let Some(link) = link.get_mut(&module) {
        link
    } else {
        let tokens = quote! {
            ::std::compile_error!("No functions to link. This call produces nothing.");
        };
        return proc_macro::TokenStream::from(tokens);
    };
    if matches!(link.state, LinkState::Written) {
        let tokens = quote! {
            ::std::compile_error!("Tried to write linked sylt functions twice.");
        };
        return proc_macro::TokenStream::from(tokens);
    }
    link.state = LinkState::Written;
    let funs: Vec<_> = link
        .mapping
        .iter()
        .map(|(ident, name, signature)| {
            let ident = proc_macro2::TokenStream::from_str(&ident).unwrap();
            quote! {
                (#name.to_string(), #ident, #signature.to_string()),
            }
        })
        .collect();

    let tokens = quote! {
        pub fn _sylt_link() -> ::std::vec::Vec<(::std::string::String, ::sylt_common::RustFunction, ::std::string::String)> {
            vec! [ #(#funs)* ]
        }
    };
    proc_macro::TokenStream::from(tokens)
}

struct SyltDoc {
    name: syn::Ident,
    comment: syn::LitStr,
    signature: syn::LitStr,
}

impl Parse for SyltDoc {
    fn parse(input: ParseStream) -> Result<Self> {
        let name: syn::Ident = input.parse()?;
        let _comma: Token![,] = input.parse()?;
        let comment = input.parse()?;
        let _comma: Token![,] = input.parse()?;
        let signature = input.parse()?;

        Ok(SyltDoc {
            name,
            comment,
            signature,
        })
    }
}

struct DocFile {
    docs: Vec<String>,
}

lazy_static! {
    static ref DOC: Arc<Mutex<DocFile>> = doc_file();
}

fn doc_file() -> Arc<Mutex<DocFile>> {
    Arc::new(Mutex::new(DocFile { docs: Vec::new() }))
}

impl DocFile {
    fn dump(&mut self) {
        use std::fs::File;
        use std::io::prelude::*;
        match File::create(&Path::new("docs/docs.json")) {
            Err(_msg) => (), // TODO(gu) report errors
            Ok(mut file) => {
                write!(file, "[\n{}\n]", self.docs.join(",\n")).unwrap();
            }
        }
    }
}

#[proc_macro_attribute]
pub fn sylt_doc(
    attrib: proc_macro::TokenStream,
    tokens: proc_macro::TokenStream,
) -> proc_macro::TokenStream {
    let doc: SyltDoc = parse_macro_input!(attrib);

    let doc = format!(
        "{{ \"name\": \"{}\", \"comment\": \"{}\", \"signature\": {}}}",
        doc.name.to_string(),
        doc.comment.value().replace("\n", "\\n"),
        doc.signature.value().split_whitespace().collect::<Vec<_>>().join(" "),
    );
    let mut doc_file = DOC.lock().unwrap();
    doc_file.docs.push(doc);
    doc_file.dump();
    drop(doc_file);

    tokens
}

struct SyltLink {
    name: syn::Ident,
    _comma: Token![,],
    module: syn::LitStr,
    __comma: Token![,],
    signature: syn::LitStr,
}

impl Parse for SyltLink {
    fn parse(input: ParseStream) -> Result<Self> {
        Ok(Self {
            name: input.parse()?,
            _comma: input.parse()?,
            module: input.parse()?,
            __comma: input.parse()?,
            signature: input.parse()?,
        })
    }
}

#[proc_macro_attribute]
pub fn sylt_link(
    attrib: proc_macro::TokenStream,
    tokens: proc_macro::TokenStream,
) -> proc_macro::TokenStream {
    let parsed: syn::ItemFn = parse_macro_input!(tokens);
    let fun = parsed.sig.ident.clone();
    let link: SyltLink = parse_macro_input!(attrib);

    let mut links = LINKS.lock().unwrap();
    let links = links
        .entry(link.module.value())
        .or_insert(ModuleLink::new());
    if matches!(links.state, LinkState::Written) {
        let tokens = quote! {
            ::std::compile_error!("Tried to write linked sylt functions twice.");
        };
        return proc_macro::TokenStream::from(tokens);
    }

    links.mapping.push((
        format!("{}::{}", link.module.value(), fun),
        link.name.to_string().clone(),
        link.signature.value().to_string(),
    ));

    let tokens = quote! {
        #parsed
    };
    proc_macro::TokenStream::from(tokens)
}
