# Benchmarks

Following benchmarks are result of `benchmark` binary. For details check out the source code.

TLDR of the methodology here is to launch Smelter in various configuration while increasing one or more of the params until we can detect frame drops.

# GPU

TODO: hardware description

Maximal capacity for different configurations

For inputs 1280x720 24fps and outputs 1920x1080;

1:1

| max input | max output | Encoder    | Decoder | Input type |
| --------- | ---------- | ---------- | ------- | ---------- |
| 5         | 5          | ultrafast  | vulkan  | 720p24fps  |
| 5         | 5          | ultrafast  | vulkan  | 1080p30fps |
| 3         | 3          | ultrafast  | vulkan  | 2160p30fps |
| 16        | 16         | ultrafast  | ffmpeg  | 720p24fps  |
| 13        | 13         | ultrafast  | ffmpeg  | 1080p30fps |
| 7         | 7          | ultrafast  | ffmpeg  | 2160p30fps |
| 3         | 3          | fast       | vulkan  | 720p24fps  |
| --        | --         | fast       | vulkan  | 1080p30fps |
| --        | --         | fast       | vulkan  | 2160p30fps |
| 1         | 1          | fast       | ffmpeg  | 720p24fps  |
| --        | --         | fast       | ffmpeg  | 1080p30fps |
| --        | --         | fast       | ffmpeg  | 2160p30fps |


2:1

| max input | max output | Encoder    | Decoder | Input type |
| --------- | ---------- | ---------- | ------- | ---------- |
| 4         | 2          | ultrafast  | vulkan  | 720p24fps  |
| 5         | 2          | ultrafast  | vulkan  | 1080p30fps |
| 3         | 1          | ultrafast  | vulkan  | 2160p30fps |
| 28        | 14         | ultrafast  | ffmpeg  | 720p24fps  |
| 18        | 9          | ultrafast  | ffmpeg  | 1080p30fps |
| 8         | 4          | ultrafast  | ffmpeg  | 2160p30fps |
| 4         | 2          | fast       | vulkan  | 720p24fps  |
| 4         | 2          | fast       | vulkan  | 1080p30fps |
| 3         | 1          | fast       | vulkan  | 2160p30fps |
| 25        | 12         | fast       | ffmpeg  | 720p24fps  |
| 10        | 5          | fast       | ffmpeg  | 1080p30fps |
| 3         | 1          | fast       | ffmpeg  | 2160p30fps |

4:1

| max input | max output | Encoder    | Decoder | Input type |
| --------- | ---------- | ---------- | ------- | ---------- |
| 4         | 1          | ultrafast  | vulkan  | 720p24fps  |
| 4         | 1          | ultrafast  | vulkan  | 1080p30fps |
| -         | -          | ultrafast  | vulkan  | 2160p30fps |
| 42        | 10         | ultrafast  | ffmpeg  | 720p24fps  |
| 20        | 5          | ultrafast  | ffmpeg  | 1080p30fps |
| 8         | 2          | ultrafast  | ffmpeg  | 2160p30fps |
| -         | -          | fast       | vulkan  | 720p24fps  |
| -         | -          | fast       | vulkan  | 1080p30fps |
| -         | -          | fast       | vulkan  | 2160p30fps |
| -         | -          | fast       | ffmpeg  | 720p24fps  |
| -0        | -          | fast       | ffmpeg  | 1080p30fps |
| -         | -          | fast       | ffmpeg  | 2160p30fps |
