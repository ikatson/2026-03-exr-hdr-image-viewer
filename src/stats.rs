const PERCENTILES: [f64; 13] = [
    10.0, 20.0, 30.0, 40.0, 50.0, 60.0, 70.0, 80.0, 90.0, 99.0, 99.5, 99.9, 100.0,
];

pub fn print_channel_stats(name: &str, data: &[f32]) {
    let mut min = f32::INFINITY;
    let mut max = f32::NEG_INFINITY;
    let mut sum = 0.0f64;
    let mut finite_count = 0usize;
    let mut non_finite_count = 0usize;

    for &v in data {
        if v.is_finite() {
            min = min.min(v);
            max = max.max(v);
            sum += f64::from(v);
            finite_count += 1;
        } else {
            non_finite_count += 1;
        }
    }

    assert!(
        finite_count > 0,
        "channel {name} contains no finite samples"
    );

    let avg = sum / finite_count as f64;
    let mut sq_diff_sum = 0.0f64;
    let mut sorted_values = Vec::with_capacity(finite_count);

    for &v in data {
        if !v.is_finite() {
            continue;
        }
        let diff = f64::from(v) - avg;
        sq_diff_sum += diff * diff;
        sorted_values.push(v);
    }
    sorted_values.sort_by(|a, b| a.total_cmp(b));

    let stddev = (sq_diff_sum / finite_count as f64).sqrt();
    println!(
        "channel={name} n={finite_count} non_finite={non_finite_count} min={min:.6} max={max:.6} avg={avg:.6} stddev={stddev:.6}"
    );
    println!("+-----+----------------+-----------+");
    println!("| pct | x (below this) | below_n   |");
    println!("+-----+----------------+-----------+");

    for &p in &PERCENTILES {
        let rank = ((p / 100.0) * (finite_count.saturating_sub(1)) as f64).round() as usize;
        let x = sorted_values[rank.min(finite_count - 1)];
        let below_n = sorted_values.partition_point(|v| *v <= x);
        println!("| {:>4.1}%| {:>14.6} | {:>9} |", p, x, below_n);
    }
    println!("+-----+----------------+-----------+");
}
