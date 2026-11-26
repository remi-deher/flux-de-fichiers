#!/usr/bin/env python3
"""Essai d'un serveur de langage Python (ADR 0034 phase 0, ADR 0032 lot b0) : complétion, infobulle et
diagnostics sur un script du contrat Forgearr, avec temps de réponse et mémoire.

Usage : ty_spike.py <script d'exemple> <commande du serveur> [arguments...]
"""

import json
import os
import queue
import subprocess
import sys
import threading
import time

REPLY_TIMEOUT_SECONDS = 20
DIAGNOSTICS_TIMEOUT_SECONDS = 15
SHOWN_LABELS = 8
WORKSPACE = "/tmp/forgearr-spike"
DOCUMENT_URI = f"file://{WORKSPACE}/script.py"

COMPLETION_QUERIES = ["sys.", "sys.st", "json.lo", "payload.", "variables.ge", "result.ke", "requests.ge"]
HOVER_WORDS = ["payload", "json.load", "variables", "working_file"]


class Client:
    def __init__(self, command):
        self.process = subprocess.Popen(command, stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
        self.messages = queue.Queue()
        self.next_id = 0
        self.stderr_lines = []
        threading.Thread(target=self.read_messages, daemon=True).start()
        threading.Thread(target=self.read_stderr, daemon=True).start()

    def read_stderr(self):
        for line in self.process.stderr:
            self.stderr_lines.append(line.decode(errors="replace").rstrip())

    def read_messages(self):
        stream = self.process.stdout
        while True:
            length = None
            while True:
                header = stream.readline()
                if not header:
                    return
                header = header.strip()
                if not header:
                    break
                name, _, value = header.decode().partition(":")
                if name.lower() == "content-length":
                    length = int(value)
            if length is None:
                continue
            body = stream.read(length)
            message = json.loads(body)
            if "method" in message and "id" in message:
                self.send({"jsonrpc": "2.0", "id": message["id"], "result": None})
            else:
                self.messages.put(message)

    def send(self, message):
        body = json.dumps(message).encode()
        self.process.stdin.write(f"Content-Length: {len(body)}\r\n\r\n".encode() + body)
        self.process.stdin.flush()

    def notify(self, method, params):
        self.send({"jsonrpc": "2.0", "method": method, "params": params})

    def request(self, method, params):
        self.next_id += 1
        request_id = self.next_id
        started = time.monotonic()
        self.send({"jsonrpc": "2.0", "id": request_id, "method": method, "params": params})
        deadline = started + REPLY_TIMEOUT_SECONDS
        pending = []
        try:
            while time.monotonic() < deadline:
                try:
                    message = self.messages.get(timeout=0.2)
                except queue.Empty:
                    continue
                if message.get("id") == request_id:
                    return message, time.monotonic() - started
                pending.append(message)
            return None, time.monotonic() - started
        finally:
            for message in pending:
                self.messages.put(message)

    def wait_diagnostics(self):
        deadline = time.monotonic() + DIAGNOSTICS_TIMEOUT_SECONDS
        kept = []
        found = None
        while time.monotonic() < deadline and found is None:
            try:
                message = self.messages.get(timeout=0.2)
            except queue.Empty:
                continue
            if message.get("method") == "textDocument/publishDiagnostics":
                found = message["params"]["diagnostics"]
            else:
                kept.append(message)
        for message in kept:
            self.messages.put(message)
        return found

    def resident_memory_mib(self):
        try:
            with open(f"/proc/{self.process.pid}/status") as status:
                for line in status:
                    if line.startswith("VmRSS:"):
                        return round(int(line.split()[1]) / 1024)
        except OSError:
            pass
        return None


def position_of(text, needle):
    index = text.index(needle)
    before = text[:index]
    return before.count("\n"), index - (before.rfind("\n") + 1)


def end_position(text):
    lines = text.split("\n")
    return len(lines) - 1, len(lines[-1])


def label_of(item):
    return item.get("label", "?")


def summarise(diagnostics):
    return [
        {"ligne": item["range"]["start"]["line"] + 1, "gravite": item.get("severity"),
         "code": item.get("code"), "message": item.get("message", "")[:120]}
        for item in diagnostics]


def hover_text(result):
    contents = (result or {}).get("contents")
    if isinstance(contents, dict):
        return contents.get("value", "")
    if isinstance(contents, list):
        return " ".join(part if isinstance(part, str) else part.get("value", "") for part in contents)
    return contents or ""


def main():
    script_path, command = sys.argv[1], sys.argv[2:]
    base_text = open(script_path, encoding="utf-8").read()
    os.makedirs(WORKSPACE, exist_ok=True)
    report = {"commande": command, "completions": {}, "infobulles": {}}
    client = Client(command)
    started = time.monotonic()
    reply, took = client.request("initialize", {
        "processId": os.getpid(),
        "rootUri": f"file://{WORKSPACE}",
        "capabilities": {"textDocument": {
            "completion": {"completionItem": {"documentationFormat": ["markdown", "plaintext"]}},
            "hover": {"contentFormat": ["markdown", "plaintext"]},
            "publishDiagnostics": {},
            "diagnostic": {"dynamicRegistration": False},
        }},
    })
    if reply is None:
        report["erreur"] = "initialize sans réponse"
        report["stderr"] = client.stderr_lines[-10:]
        print(json.dumps(report, indent=2, ensure_ascii=False))
        return 1
    report["initialisation_s"] = round(took, 2)
    report["serveur"] = reply.get("result", {}).get("serverInfo")
    capabilities = reply.get("result", {}).get("capabilities", {})
    report["declenche_la_completion_sur"] = capabilities.get("completionProvider", {}).get("triggerCharacters")
    client.notify("initialized", {})
    client.notify("textDocument/didOpen", {"textDocument": {
        "uri": DOCUMENT_URI, "languageId": "python", "version": 1, "text": base_text}})
    diagnostics = client.wait_diagnostics()
    report["delai_premiers_diagnostics_s"] = round(time.monotonic() - started, 2)
    report["diagnostics_pousses"] = None if diagnostics is None else summarise(diagnostics)
    reply, took = client.request("textDocument/diagnostic", {"textDocument": {"uri": DOCUMENT_URI}})
    pulled = (reply or {}).get("result") or {}
    report["diagnostics_demandes"] = {
        "reponse": reply is not None, "delai_s": round(took, 2), "type": pulled.get("kind"),
        "erreur": (reply or {}).get("error"), "elements": summarise(pulled.get("items", []))}
    report["capacite_diagnostics"] = capabilities.get("diagnosticProvider")
    version = 1
    for query in COMPLETION_QUERIES:
        version += 1
        text = base_text + "\n" + query
        client.notify("textDocument/didChange", {
            "textDocument": {"uri": DOCUMENT_URI, "version": version},
            "contentChanges": [{"text": text}]})
        line, character = end_position(text)
        reply, took = client.request("textDocument/completion", {
            "textDocument": {"uri": DOCUMENT_URI}, "position": {"line": line, "character": character}})
        result = (reply or {}).get("result")
        items = result.get("items", []) if isinstance(result, dict) else (result or [])
        report["completions"][query] = {
            "reponse": reply is not None, "nombre": len(items), "delai_s": round(took, 2),
            "premiers": [label_of(item) for item in items[:SHOWN_LABELS]]}
    version += 1
    client.notify("textDocument/didChange", {
        "textDocument": {"uri": DOCUMENT_URI, "version": version}, "contentChanges": [{"text": base_text}]})
    for word in HOVER_WORDS:
        line, character = position_of(base_text, word)
        reply, took = client.request("textDocument/hover", {
            "textDocument": {"uri": DOCUMENT_URI}, "position": {"line": line, "character": character + 1}})
        text = hover_text((reply or {}).get("result"))
        report["infobulles"][word] = {"reponse": reply is not None, "delai_s": round(took, 2), "texte": text[:160]}
    report["memoire_mio"] = client.resident_memory_mib()
    client.request("shutdown", None)
    client.notify("exit", None)
    try:
        client.process.wait(timeout=5)
    except subprocess.TimeoutExpired:
        client.process.kill()
    print(json.dumps(report, indent=2, ensure_ascii=False))
    return 0


if __name__ == "__main__":
    sys.exit(main())
