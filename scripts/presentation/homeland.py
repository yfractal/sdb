import numpy as np
import matplotlib.pyplot as plt

# Data
metrics = ['Avg Latency', 'CPU']
sdb = [1.13, 3.0]
rbspy = [0.05, 155]
vernier = [11.47, 2.87]
newrelic = [28.50, 3.2]

# Set up the bar width and positions
bar_width = 0.2
r1 = np.arange(len(metrics))
r2 = [x + bar_width for x in r1]
r3 = [x + bar_width for x in r2]
r4 = [x + bar_width for x in r3]

# Create figure with larger size
plt.figure(figsize=(12, 6))

# Create bars
bars1 = plt.bar(r1, sdb, bar_width, label='sdb', color='lightgreen')
bars2 = plt.bar(r2, rbspy, bar_width, label='rbspy', color='skyblue')
bars3 = plt.bar(r3, vernier, bar_width, label='vernier', color='lightpink')
bars4 = plt.bar(r4, newrelic, bar_width, label='newrelic', color='orange')

# Add value labels on top of each bar
def add_labels(bars):
    for bar in bars:
        height = bar.get_height()
        plt.text(bar.get_x() + bar.get_width()/2., height,
                f'{height:.2f}%', ha='center', va='bottom')

add_labels(bars1)
add_labels(bars2)
add_labels(bars3)
add_labels(bars4)

# Customize the plot
plt.xlabel('Metrics')
plt.ylabel('Percentage (%)')
plt.title('Performance Overhead Comparison')
plt.xticks([r + 1.5 * bar_width for r in range(len(metrics))], metrics)
plt.legend(loc='upper right')

# Adjust layout to prevent label overlap
plt.tight_layout()

# Show plot
plt.show()