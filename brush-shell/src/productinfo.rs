//! Information about this shell project.

/// The formal name of this product.
pub const PRODUCT_NAME: &str = "brush";

const PRODUCT_HOMEPAGE: &str = env!("CARGO_PKG_HOMEPAGE");
const PRODUCT_REPO: &str = env!("CARGO_PKG_REPOSITORY");

/// The URI to display as the product's homepage.
#[allow(clippy::const_is_empty)]
pub const PRODUCT_DISPLAY_URI: &str = if !PRODUCT_HOMEPAGE.is_empty() {
    PRODUCT_HOMEPAGE
} else {
    PRODUCT_REPO
};

/// Marks builds that aren't from a release (e.g. "+canary" for builds of `main`). Set through the
/// `BRUSH_VERSION_SUFFIX` environment variable at build time; empty for releases.
const PRODUCT_VERSION_SUFFIX: &str = match option_env!("BRUSH_VERSION_SUFFIX") {
    Some(suffix) => suffix,
    None => "",
};

/// The version of the product, in string form, as shown to users. Scripts see the unsuffixed
/// version through `BRUSH_VERSION`.
pub const PRODUCT_VERSION: &str =
    const_format::concatcp!(env!("CARGO_PKG_VERSION"), PRODUCT_VERSION_SUFFIX);

/// Info regarding the specific version of sources used to build this product.
pub const PRODUCT_GIT_VERSION: &str = git_version::git_version!(
    prefix = "git:",
    cargo_prefix = "cargo:",
    fallback = "unknown:",
    args = ["--always", "--dirty=-modified", "--match", ""]
);

pub(crate) fn get_product_display_str() -> String {
    std::format!(
        "{PRODUCT_NAME} version {PRODUCT_VERSION} ({PRODUCT_GIT_VERSION}) - {PRODUCT_DISPLAY_URI}"
    )
}
