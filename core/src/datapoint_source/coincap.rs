use super::assets_exchange_rate::AssetsExchangeRate;
use super::assets_exchange_rate::NanoErg;
use super::assets_exchange_rate::Usd;
use super::DataPointSourceError;

#[derive(Debug, Clone)]
pub struct CoinCap;

pub async fn get_usd_nanoerg() -> Result<AssetsExchangeRate<Usd, NanoErg>, DataPointSourceError> {
    // see https://coincap.io/assets/ergo
    let url = "https://api.coincap.io/v2/assets/ergo";
    let resp = reqwest::get(url).await?;
    let price_json = json::parse(&resp.text().await?)?;
    if let Some(p) = price_json["data"]["priceUsd"].as_str() {
        let p_float = p
            .parse::<f64>()
            .map_err(|_| DataPointSourceError::JsonMissingField {
                field: "data.priceUsd as f64".to_string(),
                json: price_json.dump(),
            })?;
        let nanoerg_per_usd = NanoErg::from_erg(1.0 / p_float);
        let rate = AssetsExchangeRate {
            per1: Usd {},
            get: NanoErg {},
            rate: nanoerg_per_usd,
        };
        Ok(rate)
    } else {
        Err(DataPointSourceError::JsonMissingField {
            field: "ergo.priceUsd as string".to_string(),
            json: price_json.dump(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::super::coingecko;
    use super::*;

    #[test]
    fn test_erg_usd_price() {
        let pair = tokio_test::block_on(get_usd_nanoerg()).unwrap();
        let coingecko = tokio_test::block_on(coingecko::get_usd_nanoerg()).unwrap();
        assert!(pair.rate > 0.0);
        let deviation_from_coingecko = (pair.rate - coingecko.rate).abs() / coingecko.rate;
        assert!(
            deviation_from_coingecko < 0.05,
            "up to 5% deviation is allowed"
        );
    }
}
