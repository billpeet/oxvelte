//! `svelte/no-unused-svelte-ignore` — disallow unused svelte-ignore comments.
//! ⭐ Recommended

use crate::linter::{LintContext, Rule};
use oxc::span::Span;

pub struct NoUnusedSvelteIgnore;

impl Rule for NoUnusedSvelteIgnore {
    fn name(&self) -> &'static str {
        "svelte/no-unused-svelte-ignore"
    }

    fn is_recommended(&self) -> bool {
        true
    }

    fn run<'a>(&self, ctx: &mut LintContext<'a>) {
        let items = crate::compiler_ignore::items(ctx);
        for item in items.iter().filter(|item| item.code.is_none()) {
            ctx.diagnostic(
                "svelte-ignore comment must include the code",
                item.token_span,
            );
        }
        if ctx.is_svelte_module
            || !ctx
                .file_path
                .as_deref()
                .is_some_and(|path| path.ends_with(".svelte"))
            || !items.iter().any(|item| item.code.is_some())
        {
            return;
        }
        let unused = match ctx.compiler_result() {
            Ok(result) if result.kind != "error" => result
                .unused_ignores
                .iter()
                // Svelte 4 does not emit this warning with generate:false.
                // Follow upstream's workaround for that compiler behavior.
                .filter(|item| {
                    !(result.compiler_version.starts_with("4.")
                        && item.code.as_deref() == Some("reactive-component"))
                })
                .cloned()
                .collect::<Vec<_>>(),
            Err(error) => {
                let message = format!("Unable to run Svelte compiler: {error}");
                ctx.diagnostic(message, Span::new(0, 0));
                return;
            }
            _ => return,
        };
        for item in unused {
            ctx.diagnostic("svelte-ignore comment is used, but not warned", item.span);
        }
    }
}
