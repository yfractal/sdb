import numpy as np
import matplotlib.pyplot as plt

# Define your data
sampling_intervals = np.array([0, 1_000, 10_000, 100_000, 1_000_000])
sdb_signal_label = np.array([7.85, 8.93, 100, np.inf, np.inf])
sdb_signal = np.array([7.85, 7.89, 8.24, 11.7, 81.40])
sdb = np.array([7.85, 7.86, 7.86, 7.91, 7.97])

# Create figure with larger size
plt.figure(figsize=(16, 8))

# Plot sdb with annotations
line1, = plt.plot(sampling_intervals, sdb[:len(sampling_intervals)], label='sdb', marker='^', color='lightgreen')
for x, y in zip(sampling_intervals, sdb[:len(sampling_intervals)]):
    plt.annotate(f'{y:.2f}', (x, y), textcoords="offset points", xytext=(0,10), ha='center')

# Plot sdb_signal with annotations
line2, = plt.plot(sampling_intervals, sdb_signal[:len(sampling_intervals)], label='sdb-signal', marker='s', color='skyblue')
for x, y in zip(sampling_intervals, sdb_signal[:len(sampling_intervals)]):
    plt.annotate(f'{y:.2f}', (x, y + 1.5), textcoords="offset points", xytext=(0,10), ha='center')

# # Plot sdb_signal_label with annotations
# line3, = plt.plot(sampling_intervals, sdb_signal_label, label='sdb-signal with label', marker='o')
# for x, y in zip(sampling_intervals, sdb_signal_label):
#     label_text = f'{y:.2f}' if not np.isinf(y) else '∞'
#     plt.annotate(label_text, (x, y), textcoords="offset points", xytext=(0,10), ha='center')

# Set the scale to logarithmic
plt.xscale('log')

# Add labels and title
plt.xlabel('Sampling Rate(Hz)')
plt.ylabel('Second')
plt.title('SDB vs SDB Signal')

# Add legend
plt.legend()

# Adjust layout to prevent label overlap
plt.tight_layout()

# Show plot
plt.show()