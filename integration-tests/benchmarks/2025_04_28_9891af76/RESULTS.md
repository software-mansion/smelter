## Benchmarks

Following benchmarks were run on AWS using `benchmarks` executable from this crate.

You can run them on your own with:
```
cargo run -r --bin benchmark -- --suite cpu  --json --json-file cpu_benchmark.json
cargo run -r --bin benchmark -- --suite full --json --json-file full_benchmark.json
```

Following benchmarks compare different EC2 instances (both CPU only and CPU+GPU).

Each of the following examples is testing max number of inputs and outputs that the instance can handle.
It provides 3 variants with different ratio of inputs to outputs. Each output renders 1, 2 or 4 inputs 
using tiles.

### `c5.xlarge` 4vCPU 8GB

- Input: 480p24fps
- Output: 480p24fps

| Input/output ratio | Encoder preset | Max inputs | Max outputs |
| ------------------ | -------------- | ---------- | ----------- |
| 1:1                | ultrafast      | **2**      | **2**       |
| 1:1                | veryfast       | **2**      | **2**       |
| 1:1                | fast           | **1**      | **1**       |
| 2:1                | ultrafast      | **2**      | **1**       |
| 2:1                | veryfast       | **2**      | **1**       |
| 2:1                | fast           | **2**      | **1**       |
| 4:1                | ultrafast      | **4**      | **1**       |
| 4:1                | veryfast       | **4**      | **1**       |
| 4:1                | fast           | **4**      | **1**       |

---

- Input: 480p24fps
- Output: 720p24fps

| Input/output ratio | Encoder preset | Max inputs | Max outputs |
| ------------------ | -------------- | ---------- | ----------- |
| 1:1                | ultrafast      | **1**      | **1**       |
| 1:1                | veryfast       | **1**      | **1**       |
| 1:1                | fast           | **1**      | **1**       |
| 2:1                | ultrafast      | **2**      | **1**       |
| 2:1                | veryfast       | **2**      | **1**       |
| 2:1                | fast           | **2**      | **1**       |
| 4:1                | ultrafast      | **-**      | **-**       |
| 4:1                | veryfast       | **-**      | **-**       |
| 4:1                | fast           | **-**      | **-**       |

---

- Input: 720p24fps
- Output: 720p24fps

It can only handle one input and one output with ultrafast preset

---

### `c5.2xlarge` 8vCPU 16GB

- Input: 480p24fps
- Output: 480p24fps

| Input/output ratio | Encoder preset | Max inputs | Max outputs |
| ------------------ | ----------     | ---------- | ----------- |
| 1:1                | ultrafast      | **4**      | **4**       |
| 1:1                | veryfast       | **3**      | **3**       |
| 1:1                | fast           | **3**      | **3**       |
| 2:1                | ultrafast      | **6**      | **3**       |
| 2:1                | veryfast       | **6**      | **3**       |
| 2:1                | fast           | **6**      | **3**       |
| 4:1                | ultrafast      | **8**      | **2**       |
| 4:1                | veryfast       | **8**      | **2**       |
| 4:1                | fast           | **4**      | **1**       |

---

- Input: 720p24fps
- Output: 720p24fps

| Input/output ratio | Encoder preset | Max inputs | Max outputs |
| ------------------ | -------------- | ---------  | ----------- |
| 1:1                | ultrafast      | **1**      | **1**       |
| 1:1                | veryfast       | **1**      | **1**       |
| 1:1                | fast           | **1**      | **1**       |
| 2:1                | ultrafast      | **2**      | **1**       |
| 2:1                | veryfast       | **2**      | **1**       |
| 2:1                | fast           | **2**      | **1**       |
| 4:1                | ultrafast      | **4**      | **1**       |
| 4:1                | veryfast       | **-**      | **-**       |
| 4:1                | fast           | **-**      | **-**       |

---

### `c5.4xlarge` 16vCPU 32GB

- Input: 480p24fps
- Output: 480p24fps

| Input/output ratio | Encoder preset | Max inputs | Max outputs |
| ------------------ | -------------- | ---------- | ----------- |
| 1:1                | ultrafast      | **6**      | **6**       |
| 1:1                | veryfast       | **6**      | **6**       |
| 1:1                | fast           | **5**      | **5**       |
| 2:1                | ultrafast      | **10**     | **5**       |
| 2:1                | veryfast       | **10**     | **5**       |
| 2:1                | fast           | **10**     | **5**       |
| 4:1                | ultrafast      | **8**      | **2**       |
| 4:1                | veryfast       | **8**      | **2**       |
| 4:1                | fast           | **4**      | **1**       |

---

- Input: 720p24fps
- Output: 720p24fps

| Input/output ratio | Encoder preset | Max inputs | Max outputs |
| ------------------ | -------------- | ---------- | ----------- |
| 1:1                | ultrafast      | **3**      | **3**       |
| 1:1                | veryfast       | **3**      | **3**       |
| 1:1                | fast           | **2**      | **2**       |
| 2:1                | ultrafast      | **6**      | **3**       |
| 2:1                | veryfast       | **4**      | **2**       |
| 2:1                | fast           | **4**      | **2**       |
| 4:1                | ultrafast      | **4**      | **1**       |
| 4:1                | veryfast       | **4**      | **1**       |
| 4:1                | fast           | **4**      | **1**       |

---

### `g4dn.xlarge` 4vCPU 16GB + GPU Nvidia T4

- Input: 480p24fps
- Output: 480p24fps

| Input/output ratio | Encoder preset | Max inputs | Max outputs |
| ------------------ | -------------- | ---------- | ----------- |
| 1:1                | ultrafast      | **24**     | **24**      |
| 1:1                | veryfast       | **13**     | **13**      |
| 1:1                | fast           | **6**      | **6**       |
| 2:1                | ultrafast      | **46**     | **23**      |
| 2:1                | veryfast       | **28**     | **14**      |
| 2:1                | fast           | **16**     | **8**       |
| 4:1                | ultrafast      | **64**     | **16**      |
| 4:1                | veryfast       | **40**     | **10**      |
| 4:1                | fast           | **20**     | **5**       |

---

- Input: 720p24fps
- Output: 720p24fps

| Input/output ratio | Encoder preset | Max inputs | Max outputs |
| ------------------ | -------------- | ---------- | ----------- |
| 1:1                | ultrafast      | **12**     | **12**      |
| 1:1                | veryfast       | **6**      | **6**       |
| 1:1                | fast           | **3**      | **3**       |
| 2:1                | ultrafast      | **26**     | **13**      |
| 2:1                | veryfast       | **14**     | **7**       |
| 2:1                | fast           | **6**      | **3**       |
| 4:1                | ultrafast      | **32**     | **8**       |
| 4:1                | veryfast       | **20**     | **5**       |
| 4:1                | fast           | **8**      | **2**       |

---

- Output: 1080p 30fps
- Input: 1080p 30fps

| Input/output ratio | Encoder preset | Max inputs | Max outputs |
| ------------------ | -------------- | ---------- | ----------- |
| 1:1                | ultrafast      | **5**      | **5**       |
| 1:1                | veryfast       | **2**      | **2**       |
| 1:1                | fast           | **1**      | **1**       |
| 2:1                | ultrafast      | **10**     | **5**       |
| 2:1                | veryfast       | **6**      | **3**       |
| 2:1                | fast           | **2**      | **1**       |
| 4:1                | ultrafast      | **16**     | **4**       |
| 4:1                | veryfast       | **8**      | **2**       |
| 4:1                | fast           | **-**      | **-**       |

---

### `c4dn.2xlarge` 16vCPU 32GB + GPU Nvidia T4

- Input: 480p24fps
- Output: 480p24fps

| Input/output ratio | Encoder preset | Max inputs | Max outputs |
| ------------------ | -------------- | ---------- | ----------- |
| 1:1                | ultrafast      | **39**     | **39**      |
| 1:1                | veryfast       | **23**     | **23**      |
| 1:1                | fast           | **12**     | **12**      |
| 2:1                | ultrafast      | **72**     | **36**      |
| 2:1                | veryfast       | **48**     | **24**      |
| 2:1                | fast           | **30**     | **15**      |
| 4:1                | ultrafast      | **80**     | **20**      |
| 4:1                | veryfast       | **64**     | **16**      |
| 4:1                | fast           | **40**     | **10**      |

---

- Input: 720p24fps
- Output: 720p24fps

| Input/output ratio | Encoder preset | Max inputs | Max outputs |
| ------------------ | -------------- | ---------- | ----------- |
| 1:1                | ultrafast      | **22**     | **22**      |
| 1:1                | veryfast       | **12**     | **12**      |
| 1:1                | fast           | **6**      | **6**       |
| 2:1                | ultrafast      | **36**     | **18**      |
| 2:1                | veryfast       | **28**     | **14**      |
| 2:1                | fast           | **14**     | **7**       |
| 4:1                | ultrafast      | **36**     | **9**       |
| 4:1                | veryfast       | **36**     | **9**       |
| 4:1                | fast           | **16**     | **4**       |

---

- Output: 1080p 30fps
- Input: 1080p 30fps

| Input/output ratio | Encoder preset | Max inputs | Max outputs |
| ------------------ | -------------- | ---------- | ----------- |
| 1:1                | ultrafast      | **10**     | **10**      |
| 1:1                | veryfast       | **4**      | **4**       |
| 1:1                | fast           | **2**      | **2**       |
| 2:1                | ultrafast      | **18**     | **9**       |
| 2:1                | veryfast       | **12**     | **6**       |
| 2:1                | fast           | **6**      | **3**       |
| 4:1                | ultrafast      | **20**     | **5**       |
| 4:1                | veryfast       | **16**     | **4**       |
| 4:1                | fast           | **4**      | **1**       |

> Fast preset for `2:1` ratio can handle 3 outputs (and 6 inputs), while for `1:1` it is only 2 outputs (and 2 inputs).
> A possible explanation for that case is that when rendering 2 inputs on an output 50% of output is uniform color, which
> might be easier to encode. Additionally, inputs are hardware decoded, so the difference between 6 vs 2 inputs might not be significant.
>
> The same effect is visible in few other cases.

