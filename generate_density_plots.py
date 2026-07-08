#!/usr/bin/env python3
import os
import csv
import numpy as np
import matplotlib.pyplot as plt

CSV_PATH = "benchmark_results.csv"
PLOTS_DIR = "plots"
os.makedirs(PLOTS_DIR, exist_ok=True)

# Set style
plt.style.use("seaborn-v0_8-whitegrid")

times = []
mems = []

if not os.path.exists(CSV_PATH):
    print(f"Error: {CSV_PATH} not found. Please run the benchmark first.")
    exit(1)

with open(CSV_PATH, mode='r', encoding='utf-8') as f:
    reader = csv.DictReader(f)
    for row in reader:
        time_str = row.get("Execution Time (ms)", "N/A").strip()
        mem_str = row.get("Peak Memory (KB)", "N/A").strip()
        if time_str != "N/A":
            try:
                times.append(float(time_str))
            except ValueError:
                pass
        if mem_str != "N/A":
            try:
                # Convert memory to Megabytes (MB) for better human readability
                mems.append(float(mem_str) / 1024.0)
            except ValueError:
                pass

# Generate execution time density plot
if times:
    plt.figure(figsize=(7, 5))
    times_arr = np.array(times)
    mu, std = np.mean(times_arr), np.std(times_arr)
    
    # Plot absolute count histogram
    counts, bins, patches = plt.hist(times_arr, bins=30, alpha=0.6, color="#1f77b4", edgecolor="#155584", label="Query Counts")
    
    # Calculate bin width to scale continuous curve to counts
    bin_width = bins[1] - bins[0]
    
    # Plot fitted normal distribution curve scaled to counts
    x = np.linspace(min(times_arr), max(times_arr), 200)
    bell_curve = (1.0 / (std * np.sqrt(2 * np.pi))) * np.exp(-0.5 * ((x - mu) / std) ** 2)
    scaled_curve = bell_curve * len(times_arr) * bin_width
    plt.plot(x, scaled_curve, linewidth=2.5, color="#d62728", label=f"Normal Fit\n($\\mu$={mu:.1f}ms, $\\sigma$={std:.1f}ms)")
    
    plt.yscale('symlog', linthresh=10)
    
    plt.title("Query Execution Time Distribution", fontsize=13, fontweight="bold")
    plt.xlabel("Execution Time (ms)", fontsize=11)
    plt.ylabel("Number of Queries", fontsize=11)
    plt.legend(fontsize=11, loc="upper right")
    plt.tight_layout()
    plt.savefig(os.path.join(PLOTS_DIR, "execution_time_density.png"), format="png", dpi=300)
    plt.close()
    print("Generated execution_time_density.png in plots/")

# Generate memory density plot
if mems:
    plt.figure(figsize=(7, 5))
    mems_arr = np.array(mems)
    mu, std = np.mean(mems_arr), np.std(mems_arr)
    
    # Plot absolute count histogram
    counts, bins, patches = plt.hist(mems_arr, bins=30, alpha=0.6, color="#2ca02c", edgecolor="#1a6e1a", label="Query Counts")
    
    # Calculate bin width to scale continuous curve to counts
    bin_width = bins[1] - bins[0]
    
    # Plot fitted normal distribution curve scaled to counts
    x = np.linspace(min(mems_arr), max(mems_arr), 200)
    bell_curve = (1.0 / (std * np.sqrt(2 * np.pi))) * np.exp(-0.5 * ((x - mu) / std) ** 2)
    scaled_curve = bell_curve * len(mems_arr) * bin_width
    plt.plot(x, scaled_curve, linewidth=2.5, color="#d62728", label=f"Normal Fit\n($\\mu$={mu:.1f}MB, $\\sigma$={std:.1f}MB)")
    
    plt.title("Query Peak Memory Usage Distribution", fontsize=13, fontweight="bold")
    plt.xlabel("Peak Memory (MB)", fontsize=11)
    plt.ylabel("Number of Queries", fontsize=11)
    plt.legend(fontsize=11, loc="upper right")
    plt.tight_layout()
    plt.savefig(os.path.join(PLOTS_DIR, "memory_usage_density.png"), format="png", dpi=300)
    plt.close()
    print("Generated memory_usage_density.png in plots/")
