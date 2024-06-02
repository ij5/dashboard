import dashboard_sys
from dataclasses import dataclass

@dataclass
class FrameData:
    action: str
    filepath: str = ""
    name: str = ""

def fetch(method, url):
    return dashboard_sys.fetch(method, url)

def print(text):
    dashboard_sys.print(text)

def send(**kwargs):
    dashboard_sys.send(FrameData(**kwargs))

def draw():
    dashboard_sys.send(FrameData(action="draw"))

def image(filepath: str):
    dashboard_sys.send(FrameData(action="image", filepath=filepath))
