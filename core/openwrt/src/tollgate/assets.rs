//! Where TollGate packages come from, and how their asset names are built.
//!
//! Kept together with the pin because the two failed together: the pin said
//! `v0.5.0` and the URL builder assumed an asset-name scheme that changed
//! underneath it. A reader needs both facts at once to see why a one-line bump
//! could not work.

/// The repository that publishes `tollgate-wrt` packages, for the v0.6.0 line
/// onwards.
///
/// This changed. Up to and including v0.5.0 the packages were attached to a
/// release on the reference-implementation repository. From v0.6.0 they are
/// published by the OpenWrt *packages* feed, which builds and signs them:
///
/// * `v0.5.0` release on `OpenTollGate/tollgate-module-basic-go` — 17 assets;
/// * `v0.6.0` release on the same repo — **zero** assets;
/// * `v0.6.0` release on `FreedomTechFeed/packages` — 26 assets.
///
/// So a bump that keeps the old repository produces a 404 for every
/// architecture, on a release tag that genuinely exists. The failure looks
/// like a network fault.
pub const FEED_REPO: &str = "FreedomTechFeed/packages";

/// The historical repository, retained so the rollback path is one constant.
///
/// The v0.5.0 assets are still live here (verified 2026-10-22), which is what
/// makes reverting the pin a safe operation rather than a rebuild.
pub const LEGACY_REPO: &str = "OpenTollGate/tollgate-module-basic-go";

/// How an asset name is spelled. Getting this wrong yields a 404 that is
/// indistinguishable from a missing release.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AssetShape {
    /// `tollgate-wrt_<v><version>_<arch>.<ext>` — the v0.2.0..v0.5.0 spelling.
    ///
    /// Arch-only, and the version keeps the leading `v` exactly as it appears
    /// in the release tag.
    LegacyArchOnly,
    /// `tollgate-wrt_<version>_<arch>-<target>.<ext>` — the v0.6.0-rc6+
    /// spelling.
    ///
    /// Target-qualified, and the version DROPS the leading `v` (the package
    /// version is `0.6.0` while the release tag is `v0.6.0`).
    FeedTargetQualified,
}

impl AssetShape {
    /// Whether this shape keeps the release tag's leading `v` in the filename.
    ///
    /// Two eras, two spellings, one URL otherwise identical in structure. This
    /// is data rather than a comment because applying a single global rule
    /// silently breaks one era or the other.
    fn keeps_leading_v(self) -> bool {
        match self {
            AssetShape::LegacyArchOnly => true,
            AssetShape::FeedTargetQualified => false,
        }
    }

    fn template(self) -> &'static str {
        match self {
            AssetShape::LegacyArchOnly => "tollgate-wrt_{version}_{arch}.{ext}",
            AssetShape::FeedTargetQualified => "tollgate-wrt_{version}_{arch}-{target}.{ext}",
        }
    }
}

/// The package name inside the released package metadata.
pub const TOLLGATE_PACKAGE_NAME: &str = "tollgate-wrt";

/// Build the download URL for one asset.
///
/// `release_tag` is the GitHub release tag (`v0.6.0`); whether the filename
/// keeps its `v` is decided by `shape`, never by the caller.
pub fn asset_url(
    repo: &str,
    release_tag: &str,
    shape: AssetShape,
    arch: &str,
    target: &str,
    ext: &str,
) -> String {
    let version = if shape.keeps_leading_v() {
        release_tag
    } else {
        release_tag.strip_prefix('v').unwrap_or(release_tag)
    };
    let filename = shape
        .template()
        .replace("{version}", version)
        .replace("{arch}", arch)
        .replace("{target}", target)
        .replace("{ext}", ext);
    format!("https://github.com/{repo}/releases/download/{release_tag}/{filename}")
}

/// Read the OpenWrt `DISTRIB_ARCH` and `DISTRIB_TARGET` a package name needs.
///
/// Both are required, and reading only the architecture is the mistake the
/// v0.6.0 URL shape punishes: one CPU arch legitimately serves several
/// targets (`mips_24kc` is both `ath79-generic` and `ath79-nand`), so an
/// arch-only name either 404s or collides.
///
/// `DISTRIB_TARGET` arrives slash-separated (`mediatek/filogic`) and the asset
/// name uses a hyphen (`mediatek-filogic`). Verified against the feed's own
/// documented read-back:
///
/// ```sh
/// . /etc/openwrt_release
/// echo "arch=${DISTRIB_ARCH} target=$(echo "$DISTRIB_TARGET" | tr / -)"
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackageTarget {
    pub arch: String,
    pub target: String,
}

impl PackageTarget {
    /// Parse the shell output of the read-back above.
    ///
    /// Tolerates the `key=value` line format and surrounding whitespace. An
    /// absent target is preserved as an empty string rather than defaulted:
    /// the caller must decide whether that is fatal, and silently guessing a
    /// target is how a wrong-but-plausible asset URL gets built.
    pub fn parse(output: &str) -> Self {
        let mut arch = String::new();
        let mut target = String::new();
        for line in output.lines() {
            let Some((key, value)) = line.split_once('=') else {
                continue;
            };
            // Both sides are trimmed: the shell may emit `arch = x86_64`
            // indented, and a real router's `openwrt_release` is hand-editable.
            match key.trim() {
                "arch" => arch = value.trim().to_string(),
                "target" => target = value.trim().replace('/', "-"),
                _ => {}
            }
        }
        PackageTarget { arch, target }
    }

    /// Whether both halves are present.
    pub fn is_complete(&self) -> bool {
        !self.arch.is_empty() && !self.target.is_empty()
    }

    /// The shell snippet that produces the lines [`parse`](Self::parse) reads.
    ///
    /// `DISTRIB_TARGET` is normalised here rather than in Rust so the value on
    /// the wire is already the asset-name spelling, matching the feed's own
    /// documented read-back.
    pub const READ_CMD: &'static str = ". /etc/openwrt_release 2>/dev/null; \
         echo \"arch=${DISTRIB_ARCH:-${OPENWRT_ARCH:-}}\"; \
         echo \"target=$(echo \\\"$DISTRIB_TARGET\\\" | tr / -)\"";
}

#[cfg(test)]
mod tests {
    use super::*;

    // ---- URL construction ------------------------------------------------

    #[test]
    fn builds_the_v060_url_shape() {
        let url = asset_url(
            FEED_REPO,
            "v0.6.0",
            AssetShape::FeedTargetQualified,
            "aarch64_cortex-a53",
            "mediatek-filogic",
            "apk",
        );
        assert_eq!(
            url,
            "https://github.com/FreedomTechFeed/packages/releases/download/v0.6.0/\
tollgate-wrt_0.6.0_aarch64_cortex-a53-mediatek-filogic.apk"
        );
    }

    #[test]
    fn builds_the_v050_url_shape_so_rollback_stays_one_constant() {
        let url = asset_url(
            LEGACY_REPO,
            "v0.5.0",
            AssetShape::LegacyArchOnly,
            "aarch64_cortex-a53",
            "",
            "apk",
        );
        assert_eq!(
            url,
            "https://github.com/OpenTollGate/tollgate-module-basic-go/releases/download/\
v0.5.0/tollgate-wrt_v0.5.0_aarch64_cortex-a53.apk"
        );
    }

    #[test]
    fn the_two_shapes_differ_in_exactly_the_version_spelling_and_the_target() {
        // Stated as a test because this pair of differences is what makes a
        // naive bump fail: same structure, two spellings.
        let legacy = asset_url(
            LEGACY_REPO,
            "v0.6.0",
            AssetShape::LegacyArchOnly,
            "x86_64",
            "",
            "apk",
        );
        let feed = asset_url(
            FEED_REPO,
            "v0.6.0",
            AssetShape::FeedTargetQualified,
            "x86_64",
            "x86-64",
            "apk",
        );
        assert!(legacy.ends_with("tollgate-wrt_v0.6.0_x86_64.apk"));
        assert!(feed.ends_with("tollgate-wrt_0.6.0_x86_64-x86-64.apk"));
    }

    #[test]
    fn a_bare_version_release_tag_still_works() {
        // Defensive: some sources spell the tag without the v.
        let url = asset_url(
            FEED_REPO,
            "0.6.0",
            AssetShape::FeedTargetQualified,
            "x86_64",
            "x86-64",
            "apk",
        );
        assert!(url.contains("/download/0.6.0/tollgate-wrt_0.6.0_x86_64-x86-64.apk"));
    }

    #[test]
    fn ipk_and_apk_differ_only_in_the_extension() {
        let apk = asset_url(
            FEED_REPO,
            "v0.6.0",
            AssetShape::FeedTargetQualified,
            "mips_24kc",
            "ath79-nand",
            "apk",
        );
        let ipk = asset_url(
            FEED_REPO,
            "v0.6.0",
            AssetShape::FeedTargetQualified,
            "mips_24kc",
            "ath79-nand",
            "ipk",
        );
        assert_eq!(apk.trim_end_matches(".apk"), ipk.trim_end_matches(".ipk"));
    }

    #[test]
    fn the_same_arch_serves_two_targets_without_colliding() {
        // The reason target qualification exists at all.
        let generic = asset_url(
            FEED_REPO,
            "v0.6.0",
            AssetShape::FeedTargetQualified,
            "mips_24kc",
            "ath79-generic",
            "ipk",
        );
        let nand = asset_url(
            FEED_REPO,
            "v0.6.0",
            AssetShape::FeedTargetQualified,
            "mips_24kc",
            "ath79-nand",
            "ipk",
        );
        assert_ne!(generic, nand, "arch-only names would collide here");
    }

    // ---- target discovery ------------------------------------------------

    #[test]
    fn parses_the_documented_read_back() {
        let out = "arch=aarch64_cortex-a53\ntarget=mediatek-filogic\n";
        let t = PackageTarget::parse(out);
        assert_eq!(t.arch, "aarch64_cortex-a53");
        assert_eq!(t.target, "mediatek-filogic");
        assert!(t.is_complete());
    }

    #[test]
    fn normalises_a_slash_separated_target() {
        // `DISTRIB_TARGET` is `mediatek/filogic`; the asset name is hyphenated.
        let out = "arch=mips_24kc\ntarget=ath79/nand\n";
        let t = PackageTarget::parse(out);
        assert_eq!(t.target, "ath79-nand");
    }

    #[test]
    fn tolerates_surrounding_whitespace_and_noise() {
        let out = "  arch = x86_64  \nsome other line\ntarget = x86/64\n";
        let t = PackageTarget::parse(out);
        assert_eq!(t.arch, "x86_64");
        assert_eq!(t.target, "x86-64");
    }

    #[test]
    fn an_absent_target_is_empty_rather_than_guessed() {
        // Must NOT default to something plausible: a guessed target builds a
        // wrong-but-valid-looking URL, which is worse than a loud failure.
        let t = PackageTarget::parse("arch=x86_64\n");
        assert_eq!(t.arch, "x86_64");
        assert_eq!(t.target, "");
        assert!(!t.is_complete());
    }

    #[test]
    fn the_read_command_asks_for_both_halves() {
        // Guards against someone "simplifying" this back to arch-only.
        assert!(PackageTarget::READ_CMD.contains("DISTRIB_ARCH"));
        assert!(PackageTarget::READ_CMD.contains("DISTRIB_TARGET"));
        assert!(PackageTarget::READ_CMD.contains("tr / -"));
    }
}
