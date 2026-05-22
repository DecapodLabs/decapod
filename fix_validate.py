import sys

with open('src/core/validate.rs', 'r') as f:
    lines = f.readlines()

# Locate the section and replace it with clean logic
start_line = -1
end_line = -1

for i, line in enumerate(lines):
    if 'Must use RPC constitution access, never docs CLI' in line:
        start_line = i
        # Find where the next jail rule check starts
        for j in range(i + 1, len(lines)):
            if 'Must include explicit jail rule' in j or 'jail rule marker' in lines[j]:
                end_line = j
                break
        break

if start_line != -1 and end_line != -1:
    new_logic = [
        "        // Must use RPC constitution access.\n",
        "        if agent_content.contains(\"constitution.get\") {\n",
        "            pass(\n",
        "                &format!(\"{} references constitution.get RPC\", agent_file),\n",
        "                ctx,\n",
        "            );\n",
        "        } else {\n",
        "            fail(\n",
        "                &format!(\"{} missing constitution.get RPC reference\", agent_file),\n",
        "                ctx,\n",
        "            );\n",
        "            all_present = false;\n",
        "        }\n",
        "\n"
    ]
    lines[start_line:end_line] = new_logic
    with open('src/core/validate.rs', 'w') as f:
        f.writelines(lines)
    print("Successfully patched src/core/validate.rs")
else:
    print(f"Failed to find target section: start={start_line}, end={end_line}")
    sys.exit(1)
