use std::collections::BTreeMap;

pub fn otsu_threshold(samples: &[u64]) -> Option<u64> {
    if samples.is_empty() {
        return None;
    }

    let mut histogram = BTreeMap::new();
    for &sample in samples {
        *histogram.entry(sample).or_insert(0usize) += 1;
    }

    if histogram.len() < 2 {
        return None;
    }

    let total = samples.len() as f64;
    let total_sum: f64 = histogram
        .iter()
        .map(|(&value, &count)| value as f64 * count as f64)
        .sum();
    let mut weight_low = 0.0;
    let mut sum_low = 0.0;
    let mut best_threshold = None;
    let mut best_variance = -1.0;

    for (&value, &count) in &histogram {
        weight_low += count as f64;
        let weight_high = total - weight_low;
        if weight_high <= 0.0 {
            break;
        }

        sum_low += value as f64 * count as f64;
        let mean_low = sum_low / weight_low;
        let mean_high = (total_sum - sum_low) / weight_high;
        let difference = mean_low - mean_high;
        let variance = weight_low * weight_high * difference * difference;

        if variance > best_variance {
            best_variance = variance;
            best_threshold = Some(value);
        }
    }

    best_threshold
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_samples_have_no_threshold() {
        assert_eq!(otsu_threshold(&[]), None);
    }

    #[test]
    fn uniform_samples_have_no_threshold() {
        assert_eq!(otsu_threshold(&[5, 5, 5]), None);
    }

    #[test]
    fn bimodal_samples_split_at_low_cluster_edge() {
        assert_eq!(
            otsu_threshold(&[10, 10, 10, 10, 100, 100, 100, 100]),
            Some(10)
        );
    }

    #[test]
    fn skewed_samples_prefer_small_cluster() {
        assert_eq!(otsu_threshold(&[5, 5, 5, 100]), Some(5));
    }

    #[test]
    fn three_clusters_split_before_largest_gap() {
        assert_eq!(otsu_threshold(&[1, 1, 2, 2, 10, 10]), Some(2));
    }
}
