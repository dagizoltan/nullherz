import sys

with open('crates/audio-dsp/src/lib.rs', 'r') as f:
    lines = f.readlines()

new_lines = []
in_unsafe_fn = False
brace_count = 0
for line in lines:
    if 'pub unsafe fn' in line and '{' in line:
        new_lines.append(line)
        new_lines.append('        unsafe {\n')
        in_unsafe_fn = True
        brace_count = 1
        continue

    if in_unsafe_fn:
        brace_count += line.count('{')
        brace_count -= line.count('}')
        if brace_count == 0:
            new_lines.append('        }\n')
            new_lines.append(line)
            in_unsafe_fn = False
            continue

    new_lines.append(line)

with open('crates/audio-dsp/src/lib.rs', 'w') as f:
    f.writelines(new_lines)
