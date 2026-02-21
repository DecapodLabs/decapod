#!/usr/bin/env python3
import json
import subprocess
import uuid
from dataclasses import dataclass
from typing import Any, Dict, List, Optional


@dataclass
class RpcResponse:
    id: str
    success: bool
    raw: Dict[str, Any]


def run_rpc(op: str, params: Optional[Dict[str, Any]] = None, request_id: Optional[str] = None) -> RpcResponse:
    payload = {
        "id": request_id or str(uuid.uuid4()),
        "op": op,
        "params": params or {},
    }
    proc = subprocess.run(
        ["decapod", "rpc", "--stdin"],
        input=json.dumps(payload).encode("utf-8"),
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        check=False,
    )
    if proc.returncode != 0:
        raise RuntimeError(proc.stderr.decode("utf-8", errors="replace").strip())
    raw = json.loads(proc.stdout.decode("utf-8"))
    return RpcResponse(id=raw.get("id", payload["id"]), success=bool(raw.get("success", False)), raw=raw)


def run_validate() -> Dict[str, Any]:
    proc = subprocess.run(["decapod", "validate"], stdout=subprocess.PIPE, stderr=subprocess.PIPE, check=False)
    return {
        "envelope_version": "1.0.0",
        "cmd": "validate",
        "status": "ok" if proc.returncode == 0 else "error",
        "exit_code": proc.returncode,
        "stdout": proc.stdout.decode("utf-8", errors="replace"),
        "stderr": proc.stderr.decode("utf-8", errors="replace"),
    }
