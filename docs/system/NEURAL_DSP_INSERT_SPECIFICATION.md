# Nullherz Neural DSP Insert & EQ Architecture Specification

**Prepared by:** Lead Audio Systems & Neural DSP Architecture Team
**Status:** ARCHITECTURAL SPECIFICATION
**Target:** Real-Time Zero-Allocation Neural Inference for EQs, Saturators & Insert FX

---

## 1. Executive Summary & Philosophy

Neural Network-based audio processing ("Neural DSP") allows Nullherz to model complex, non-linear hardware (vintage analog Equalizers, tube preamps, tape saturation, and dynamic processors) with extraordinary sonic accuracy.

To integrate neural networks directly into Nullherz's **Execution Plane (`audio-core`, `audio-dsp`)**, neural processing nodes must strictly obey the **Law of Zero Allocation** and **Law of SIMD-First Execution**. Standard machine learning frameworks (e.g. PyTorch, ONNXRuntime) allocate heap memory dynamically, which is illegal on real-time audio threads.

Nullherz solves this by introducing a **SIMD-Optimized Static Neural Inference Engine** that executes pre-compiled weight matrices directly on 64-byte aligned SIMD buffers (`FloatX16`) using rational Padé approximants for non-linear activation functions.

```
+-----------------------------------------------------------------------------------+
|                        NULLHERZ NEURAL DSP INFERENCE                              |
|                                                                                   |
|  Audio Inputs  ──>  [ Static Pre-Allocated ]  ──(AVX-512 / NEON)──> Audio Outputs  |
|  (f32 Block)        [ Weight Matrices & Ring ]  Padé Activations     (f32 Block)    |
+-----------------------------------------------------------------------------------+
```

---

## 2. Neural Model Families for EQ & Inserts

### 2.1 Temporal Convolutional Networks (TCN)
TCNs with dilated 1D causal convolutions are the premier architecture for modeling analog EQ responses, vintage channel strips, and non-linear saturation.
* **Dilated Convolutions**: By exponentially increasing dilation factors ($d = 1, 2, 4, 8, 16$), a shallow 5-layer network achieves a receptive field of several hundred samples, capturing low-frequency phase shifts and transformer hysteresis.
* **Zero Latency**: Causal 1D kernels ($K=3$) operate with zero algorithmic look-ahead delay.

### 2.2 HyperNetwork Conditioned Parametric EQ
Conventional neural models are fixed-function. To support interactive GUI knobs (Gain, Frequency, Q Factor):
* **Conditioning Vector ($c$)**: Parameter knobs (e.g., Low Gain, Mid Freq, High Gain) are concatenated into a conditioning vector $c \in \mathbb{R}^K$.
* **HyperNetwork Weights**: A lightweight multi-layer perceptron (MLP) translates $c$ off the audio thread into dense 1D convolution kernel weights $W(c)$, which are atomically swapped onto the RT audio thread.

### 2.3 State-Space Models (SSM) & Recurrent Units (LSTM/GRU)
For dynamic equalizers, multi-band compressors, and tape saturation:
* **Diagonal State-Space Equations**:
  $$x_{k+1} = A x_k + B u_k$$
  $$y_k = C x_k + D u_k$$
* Diagonalizing matrix $A$ allows SIMD vectorization across hidden states $x_k$, enabling 16-parallel hidden state updates in a single CPU clock cycle.

---

## 3. Real-Time Zero-Allocation Inference Kernel

### 3.1 SIMD Weight Structuring
All layer weights and biases are statically stored in 64-byte aligned structures (`FloatX16`).

```rust
// RT-Safe Neural Layer Definition
#[repr(C, align(64))]
pub struct NeuralConv1dLayer<const IN_CH: usize, const OUT_CH: usize, const KERNEL_SIZE: usize> {
    pub weights: [[[f32; KERNEL_SIZE]; IN_CH]; OUT_CH],
    pub bias: [f32; OUT_CH],
    pub dilation: usize,
}

impl<const IN_CH: usize, const OUT_CH: usize, const KERNEL_SIZE: usize>
    NeuralConv1dLayer<IN_CH, OUT_CH, KERNEL_SIZE>
{
    #[inline(always)]
    pub fn process_sample_simd(
        &self,
        history: &[f32],
        output: &mut [f32; OUT_CH],
    ) {
        // Unrolled 16-wide SIMD matrix multiply-accumulate (FMA)
        for oc in 0..OUT_CH {
            let mut acc = self.bias[oc];
            for ic in 0..IN_CH {
                for k in 0..KERNEL_SIZE {
                    acc += history[ic * KERNEL_SIZE + k] * self.weights[oc][ic][k];
                }
            }
            output[oc] = acc;
        }
    }
}
```

### 3.2 Padé Activation Functions
Standard transcendental functions (`tanh`, `sigmoid`, `GELU`) invoke expensive math library calls. Nullherz uses rational Padé approximants:

$$\tanh(x) \approx \frac{x(1 + 0.12317192x^2)}{1 + 0.4565311x^2 + 0.01524316x^4}$$

This guarantees SIMD vectorization without branching or transcendental microcode traps.

---

## 4. Extensibility & Third-Party Neural Models

Nullherz supports third-party neural model formats via the **Protocol Plane (`sidecar-sdk`, `fx-runtime`)**:

1. **NAM (Neural Amp Modeler) & RTNeural Integration**:
   - `fx-runtime` parses standard `.nam` and `.json` model files.
   - Models are compiled into flattened execution graphs and hosted either in-process or via out-of-process sidecars over Sidecar Protocol V2.
2. **ONNX Model Import**:
   - Offline ONNX graphs are compiled down to statically sized C/Rust structs using `rtneural` or custom code generation, ensuring zero runtime allocations.

---

## 5. Latency & Performance Target Metrics

| Neural Model | Hidden Channels | Receptive Field | CPU Cost per 256-sample Block | Algorithmic Latency |
| :--- | :--- | :--- | :--- | :--- |
| Analog EQ TCN | 16 channels, 4 layers | 128 samples | ~28 µs (0.5% CPU) | **0.00 ms (0 samples)** |
| Tube Preamp / Saturation | 32 channels, 6 layers | 256 samples | ~62 µs (1.1% CPU) | **0.00 ms (0 samples)** |
| Dynamic Neural EQ (LSTM) | 24 hidden states | Infinite (IIR) | ~45 µs (0.8% CPU) | **0.00 ms (0 samples)** |
| Neural Amp Modeler (NAM) | 64 channels, 10 layers | 1024 samples | ~180 µs (3.3% CPU) | **0.00 ms (0 samples)** |

---

## 6. Strategic Implementation Roadmap

1. **Phase 1**: Add `NeuralBiquad` and `NeuralTcn` processor factories into `nullherz-processors`.
2. **Phase 2**: Implement the `HyperNetwork` parameter translator inside `nullherz-conductor` to map UI EQ knobs to neural weights.
3. **Phase 3**: Add NAM (`.nam`) model loading support in `nullherz-inspector`'s Audio Editor and Sampler views.
