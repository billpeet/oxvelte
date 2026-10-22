# Enable robust css property awareness

### Preface:
There is an existing npm package `@mdn/browser-compat-data` which already has an extensive catalogue of CSS properties.
It is used by other open source projects like [nikolai-shabalin/css-properties](https://nikolai-shabalin.github.io/css-properties/en/)` for the sole purpose of identifying css props for that identification and docs search site.

### Goal:
Rather than manually maintaining a json list of these props, via the upstream's `src/linter/rules/known_css_properties_0_37_0.json`, this is something we can generate programatically.
In fact, we shouldn't need to create this file at all, as we can leverage a `build.rs` explicitly defined in our `Cargo.toml` that when triggered via our standard `cargo build --release` can look inside of `node_modules/@mdn/browser-compat-data/css/` upon which the sub-dir's `properties/`, `selectors/`, `types/`, & `at-rules/` each contains the majority of the props already listed inside `src/linter/rules/known_css_properties_0_37_0.json`, as well as others.
The file names in those paths are the actual prop names, in the proper css prop syntax, thus we need only fetch the filnames, stip the `.json` extension, and we have a vendored list of css props.
The idea here would be to pass the json props list built by `build.rs` to `src/linter/rules/no_unknown_style_directive_property.rs`'s function `known_css_properties` via the `include_str!` macro, thus inhibiting the manually maintained `known_css_properties_0_37_0.json` copy. Example:

```rust

fn known_css_properties() -> &'static FxHashSet<String> {
    static PROPERTIES: OnceLock<FxHashSet<String>> = OnceLock::new();
    PROPERTIES.get_or_init(|| {
        let data: KnownCssProperties = serde_json::from_str(include_str!(concat!(
            env!("OUT_DIR"),
            "/known_css_properties.json"
        )))
        .expect("Generated known_css_properties.json data should be valid JSON");
        
        data.properties.into_iter().collect()
    })
}

```

### Planning:
This creates a direct build-time dependency upon `@mdn/browser-compat-data` which may not be ideal, so two alternatives methods to solve this are:
1. Forgo the npm package install and source the filenames from the repo itself at `github.com/mdn/browser-compat-data`. For example, we use the github api URLs:
    - https://api.github.com/repos/mdn/browser-compat-data/contents/css/selectors
    - https://api.github.com/repos/mdn/browser-compat-data/contents/css/properties
    - https://api.github.com/repos/mdn/browser-compat-data/contents/css/types
    - https://api.github.com/repos/mdn/browser-compat-data/contents/css/at-rules

  This will return a json response of the dir's files list of which we only care about the `names` keys. However, github limits to 60 reqs per hour so to avoid requiring appending the user's github bearer token, it would be a good idea to cache the generated output json file in a secondary dir other than the env `OUT_DIR` (see `known_css_properties` example above). This only really needs to be ran once and then its contents are not subject to change. We have our `build.rs` check to see if the cached copy already exists in a secondary staging dir (that we maintain) and simply copy it to `OUT_DIR` if it already exists. If it does not, then it proceeds with the `api.github.com` requests for json generation.

2. We stick to using the `npm` dependency approach, which is very similar to the way other testing fixtures, tests, and/or scripts under the original upstream `github.com/tolgaouz/oxvelte` repo were sourced. Our `build.rs` simply reads the filenames under `node_modules/@mdn/browser-compat-data/css/`, stripping the `.json` file extensions, to identify the css properties and generate the same generated output `known_css_properties.json` to `OUT_DIR`.

### Other Info:
To ensure we can provide minimal release tarballs of the project source code, so that we don't have to include a static copy of `@mdn/browser-compat-data`, we would need `build.rs` to pull from a cached copy of the generated `known_css_properties.json`, if it exists in the build env context. To be clear, `known_css_properties.json` will excluded from the repo via `.gitignore`, but during the source release tarball creation, our `cargo build` smoketest will ensure that a cached copy gets added to a secondary directory that `build.rs` should be configured to fetch the generated json from first. Only, when that json doesn't exist does it commense its json generation steps. Thus, during the github workflow, the `release.sh` executes `cargo build --release` at which the `known_css_properties.json` does not yet exist. `build.rs` will create it though and save it both the env's `OUT_DIR` context, and our explicitly added cache dir. Thus, the compiled releases will have their copy of `known_css_properties.json` embedded, and by the time `tar` archives the `src/` dir, `Cargo.lock`, `Cargo.toml`, etc..., we include our secondary path to the cached `known_css_properties.json` in that process. This ensures that package managers like `homebrew` can build `oxvelte` form a minimal tarball release, without needing any additional web requests or deps.

### Final Thoughts 21OCT2026:
It is possible that `mdn/browser-compat-data` may not contain every css property that is already defined insided the manually maintained `src/linter/rules/known_css_properties_0_37_0.json`. Therefore, we may find other npm packages we can source these props from, but its also possible that a mix of #1 and #2 from 'Planning' above may be necessary to make this approach viable, hence another reason why locally cacheing `known_css_properties.json`, and NOT only relying on `cargo`'s build context is beneficial.
