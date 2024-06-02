import dashboard_sys
import json
from dataclasses import dataclass
from typing import Union

@dataclass
class FrameData:
    action: str
    value: str

def fetch(method, url):
    return dashboard_sys.fetch(method, url)

def print(text):
    dashboard_sys.print(text)

def send(action: str, value: dict):
    dashboard_sys.send(FrameData(action=action, value=json.dumps(value, ensure_ascii=False)))

def image(imagename: str, filepath: str):
    send(action="image", value=dict(
        name=imagename,
        filepath=filepath,
    ))

def text(text: str, *, color: str = "white"):
    send(action="text", value=dict(text=text, color=color))
