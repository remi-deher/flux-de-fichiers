import json
import sys

import requests

payload = json.load(sys.stdin)
working_file = payload["workingFile"]
variables = payload.get("variables", {})
total = undefined_name + 1
result = {"status": "success", "message": working_file, "variables": variables}
json.dump(result, sys.stdout)
