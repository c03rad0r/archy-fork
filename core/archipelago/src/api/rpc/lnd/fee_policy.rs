//! Explicit on-chain fee choices retain priority; omitted choices target the next block.
use anyhow::{ensure, Context, Result};
use serde_json::Value;

pub(super) const DEFAULT_TARGET: i64 = 1;

pub(super) fn estimated_sat_per_vbyte(value: &Value) -> Result<u64> {
    let per_kw = value["sat_per_kw"]
        .as_u64()
        .or_else(|| value["sat_per_kw"].as_str().and_then(|s| s.parse().ok()))
        .context("Next-block fee estimate is unavailable")?;
    let rate = per_kw.div_ceil(250);
    ensure!(
        (1..=5000).contains(&rate),
        "Next-block fee estimate is outside supported bounds; choose an explicit fee"
    );
    Ok(rate)
}

pub(super) fn fee_options(params: &Value) -> Result<(Option<i64>, Option<i64>)> {
    let integer = |key: &str, max: i64| -> Result<Option<i64>> {
        match params.get(key) {
            None | Some(Value::Null) => Ok(None),
            Some(value) => {
                let n = value
                    .as_i64()
                    .with_context(|| format!("{key} must be a positive whole number"))?;
                ensure!((1..=max).contains(&n), "{key} must be between 1 and {max}");
                Ok(Some(n))
            }
        }
    };
    let target = integer("target_conf", 1008)?;
    let rate = integer("sat_per_vbyte", 5000)?;
    ensure!(
        target.is_none() || rate.is_none(),
        "Specify either target_conf or sat_per_vbyte, not both"
    );
    Ok((
        if rate.is_none() {
            Some(target.unwrap_or(DEFAULT_TARGET))
        } else {
            None
        },
        rate,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn estimates_round_up_and_missing_or_extreme_estimates_fail_closed() {
        assert_eq!(
            estimated_sat_per_vbyte(&json!({"sat_per_kw":"501"})).unwrap(),
            3
        );
        assert_eq!(
            estimated_sat_per_vbyte(&json!({"sat_per_kw":250})).unwrap(),
            1
        );
        for v in [
            json!({}),
            json!({"sat_per_kw":0}),
            json!({"sat_per_kw":-1}),
            json!({"sat_per_kw":1250001}),
        ] {
            assert!(estimated_sat_per_vbyte(&v).is_err());
        }
    }

    #[test]
    fn next_block_default_preserves_explicit_slower_and_custom_choices() {
        assert_eq!(fee_options(&json!({})).unwrap(), (Some(1), None));
        assert_eq!(
            fee_options(&json!({"target_conf":null})).unwrap(),
            (Some(1), None)
        );
        for target in [1, 3, 6, 144, 1008] {
            assert_eq!(
                fee_options(&json!({"target_conf":target})).unwrap(),
                (Some(target), None)
            );
        }
        for rate in [1, 17, 5000] {
            assert_eq!(
                fee_options(&json!({"sat_per_vbyte":rate})).unwrap(),
                (None, Some(rate))
            );
        }
    }

    #[test]
    fn malformed_explicit_fees_never_silently_become_fast() {
        for value in [
            json!(0),
            json!(-1),
            json!(1.5),
            json!("6"),
            json!(true),
            json!({}),
            json!(1009),
        ] {
            assert!(fee_options(&json!({"target_conf":value})).is_err());
        }
        for value in [
            json!(0),
            json!(-1),
            json!(1.5),
            json!("6"),
            json!(true),
            json!(5001),
        ] {
            assert!(fee_options(&json!({"sat_per_vbyte":value})).is_err());
        }
        assert!(fee_options(&json!({"target_conf":1,"sat_per_vbyte":2})).is_err());
    }
}
