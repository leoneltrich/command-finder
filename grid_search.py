#!/usr/bin/env python3
import os
import sys
import csv
import json
import subprocess
import re

# Mathematical metric formulas implemented in this script:
# Precision = |P intersect E| / |P|
# Recall = |P intersect E| / |E|
# F1 = 2 * (Precision * Recall) / (Precision + Recall)
# FLAGS/DEST = 1.0 if |P| == 0 and |E| == 0 else max(|P|/|E|, |E|/|P|) if |P| > 0 and |E| > 0 else ||E| - |P|| + 1.0
# Recall/Factor = Recall / (FLAGS/DEST)

def compute_metrics(predicted, expected):
    p_set = set(predicted)
    e_set = set(expected)
    
    p_len = len(p_set)
    e_len = len(e_set)
    intersection_len = len(p_set.intersection(e_set))
    
    # 1. Precision
    if p_len == 0:
        precision = 1.0 if e_len == 0 else 0.0
    else:
        precision = intersection_len / p_len
        
    # 2. Recall
    if e_len == 0:
        recall = 1.0 if p_len == 0 else 0.0
    else:
        recall = intersection_len / e_len
        
    # 3. F1-Score
    if precision + recall > 0:
        f1 = 2 * (precision * recall) / (precision + recall)
    else:
        f1 = 0.0
        
    # 4. Symmetric Option Count Error (Factor / FLAGS/DEST)
    if e_len > 0 and p_len > 0:
        ratio = p_len / e_len
        factor = ratio if ratio >= 1.0 else 1.0 / ratio
    elif e_len == 0 and p_len == 0:
        factor = 1.0
    else:
        factor = abs(e_len - p_len) + 1.0
        
    # 5. Recall / Factor Metric
    recall_factor = recall / factor
    
    return {
        "precision": precision,
        "recall": recall,
        "f1": f1,
        "factor": factor,
        "recall_factor": recall_factor
    }

def print_table(title, headers, rows):
    print(f"\n{title}")
    # Compute column widths
    widths = [len(h) for h in headers]
    for row in rows:
        for idx, val in enumerate(row):
            widths[idx] = max(widths[idx], len(str(val)))
            
    header_str = " | ".join(f"{str(h).ljust(widths[idx])}" for idx, h in enumerate(headers))
    print(header_str)
    print("-+-".join("-" * w for w in widths))
    for row in rows:
        row_str = " | ".join(f"{str(val).ljust(widths[idx])}" for idx, val in enumerate(row))
        print(row_str)

def main():
    dry_run = "--dry-run" in sys.argv or "-d" in sys.argv
    
    print("Compiling Rust command-finder in release mode...")
    build_res = subprocess.run(["cargo", "build", "--release"], capture_output=True)
    if build_res.returncode != 0:
        print("Compilation failed!", file=sys.stderr)
        print(build_res.stderr.decode(), file=sys.stderr)
        sys.exit(1)
        
    binary_path = "./target/release/command-finder"
    if not os.path.exists(binary_path):
        print(f"Binary not found at {binary_path}!", file=sys.stderr)
        sys.exit(1)
        
    csv_path = "validation_set_with_options.csv"
    if not os.path.exists(csv_path):
        print(f"Validation set file not found: {csv_path}!", file=sys.stderr)
        sys.exit(1)
        
    # Load dataset
    dataset = []
    with open(csv_path, mode="r", encoding="utf-8") as f:
        reader = csv.DictReader(f)
        for row in reader:
            query = row["Original Text"]
            expected_opts = json.loads(row["options"])
            dataset.append({
                "query": query,
                "expected": expected_opts
            })
            
    if dry_run:
        print("\n=== RUNNING DRY RUN ===")
        print("Evaluating on the first 5 validation queries.")
        dataset = dataset[:5]
        # Extremely small search space for dry run validation
        post_alphas = [0.60]
        post_multipliers = [1.00]
    else:
        # Full parameter search space for rank aggregator post Otsu cutoff parameters
        post_alphas = [0.5, 0.6, 0.7, 0.8, 0.9, 1.0, 1.1, 1.2, 1.3, 1.4, 1.5]
        post_multipliers = [1.0, 1.5, 2.0, 2.5, 3.0]
        
    option_regex = re.compile(r"Option:\s+([^\s\(\)]+)")
    
    total_combinations = len(post_alphas) * len(post_multipliers)
    print(f"Total parameter combinations to sweep: {total_combinations}")
    print(f"Total validation queries per combination: {len(dataset)}")
    
    grid_results = []
    
    counter = 0
    # Sweep parameter grid
    for post_a in post_alphas:
        for post_m in post_multipliers:
            counter += 1
            if dry_run:
                print(f"Sweeping combination {counter}/{total_combinations}...")
            else:
                if counter % 5 == 0 or counter == total_combinations:
                    print(f"Progress: {counter}/{total_combinations} combinations evaluated.")
                    
            # Set environment variables for subprocess execution
            env = os.environ.copy()
            # Fixed weights as provided by the user
            env["KEYWORD_OPTION_WEIGHT"] = "0.4618"
            env["EMBEDDING_OPTION_WEIGHT"] = "0.5382"
            # Tuned engine thresholds are untouched (meaning engines default to their internal configs)
            env["POST_ALPHA"] = str(post_a)
            env["POST_MULTIPLIER"] = str(post_m)
            
            # Accumulators for this configuration
            sum_precision = 0.0
            sum_recall = 0.0
            sum_f1 = 0.0
            sum_factor = 0.0
            sum_recall_factor = 0.0
            
            for item in dataset:
                proc = subprocess.run(
                    [binary_path, "query", item["query"]],
                    env=env,
                    capture_output=True,
                    text=True
                )
                
                # Parse predicted options from stdout
                predicted = option_regex.findall(proc.stdout)
                
                metrics = compute_metrics(predicted, item["expected"])
                sum_precision += metrics["precision"]
                sum_recall += metrics["recall"]
                sum_f1 += metrics["f1"]
                sum_factor += metrics["factor"]
                sum_recall_factor += metrics["recall_factor"]
                
            n = len(dataset)
            grid_results.append({
                "params": {
                    "post_a": post_a,
                    "post_m": post_m
                },
                "avg_precision": sum_precision / n,
                "avg_recall": sum_recall / n,
                "avg_f1": sum_f1 / n,
                "avg_factor": sum_factor / n,
                "avg_recall_factor": sum_recall_factor / n
            })
                                    
    # Prepare tables for optimization reports
    headers = [
        "POST_ALPHA", "POST_MULTIPLIER",
        "Avg Precision", "Avg Recall", "Avg F1", "Avg Factor", "Avg Recall/Factor"
    ]
    
    def result_to_row(r):
        p = r["params"]
        return [
            f"{p['post_a']:.2f}", f"{p['post_m']:.2f}",
            f"{r['avg_precision']:.4f}", f"{r['avg_recall']:.4f}", f"{r['avg_f1']:.4f}",
            f"{r['avg_factor']:.4f}", f"{r['avg_recall_factor']:.4f}"
        ]
        
    # 1. Optimized for Recall (Higher is Better)
    results_opt_recall = sorted(grid_results, key=lambda x: x["avg_recall"], reverse=True)
    recall_rows = [result_to_row(r) for r in results_opt_recall[:3]]
    print_table("Top 3 Configurations Optimized for Recall (Higher is Better)", headers, recall_rows)
    
    # 2. Optimized for Factor Metric (Lower is Better)
    results_opt_factor = sorted(grid_results, key=lambda x: x["avg_factor"])
    factor_rows = [result_to_row(r) for r in results_opt_factor[:3]]
    print_table("Top 3 Configurations Optimized for FLAGS/DEST Factor Metric (Lower is Better)", headers, factor_rows)
    
    # 3. Optimized for Recall/Factor Metric (Higher is Better)
    results_opt_recall_factor = sorted(grid_results, key=lambda x: x["avg_recall_factor"], reverse=True)
    recall_factor_rows = [result_to_row(r) for r in results_opt_recall_factor[:3]]
    print_table("Top 3 Configurations Optimized for Recall/Factor Metric (Higher is Better)", headers, recall_factor_rows)
    
    if dry_run:
        print("\nDry run completed successfully.")
        print("To run the full grid search on the entire validation dataset, execute:")
        print("  python3 grid_search.py")

if __name__ == "__main__":
    main()
