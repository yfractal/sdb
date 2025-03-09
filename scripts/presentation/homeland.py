import numpy as np
import matplotlib.pyplot as plt

# Data
metrics = ['Avg Latency', 'P99 Latency', 'CPU']
sdb_signal = [2.4, 20.6, 2.88]
sdb = [0.4, 7.7, 3.89]

# Set up the bar width and positions
bar_width = 0.35
r1 = np.arange(len(metrics))
r2 = [x + bar_width for x in r1]

# Create figure with larger size
plt.figure(figsize=(10, 6))

# Create bars
bars1 = plt.bar(r1, sdb_signal, bar_width, label='sdb-signal', color='skyblue')
bars2 = plt.bar(r2, sdb, bar_width, label='sdb', color='lightgreen')

# Add value labels on top of each bar
def add_labels(bars):
    for bar in bars:
        height = bar.get_height()
        plt.text(bar.get_x() + bar.get_width()/2., height,
                f'{height}%', ha='center', va='bottom')

add_labels(bars1)
add_labels(bars2)

# Customize the plot
plt.xlabel('Metrics')
plt.ylabel('Percentage (%)')
plt.title('Overhead: sdb vs sdb-signal(1000 Hz)')
plt.xticks([r + bar_width/2 for r in range(len(metrics))], metrics)
plt.legend()

# Adjust layout to prevent label overlap
plt.tight_layout()

# Show plot
plt.show()