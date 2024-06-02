import dashboard_sys
import json
from dataclasses import dataclass
from typing import Union

@dataclass
class FrameData:
    action: str
    name: str
    value: str

def fetch(method, url):
    return dashboard_sys.fetch(method, url)

def print(text):
    dashboard_sys.print(text)

def send(action: str, name: str, value: dict):
    dashboard_sys.send(FrameData(action=action, name=name, value=json.dumps(value, ensure_ascii=False)))

def image(name: str, filepath: str):
    send(action="image", name=name, value=dict(
        filepath=filepath,
    ))

def text(name: str, text: str, *, color: str = "white"):
    send(action="text", name=name, value=dict(text=text, color=color))
