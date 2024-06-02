import dashboard_sys
from dataclasses import dataclass

@dataclass
class FrameData:
    action: str

def fetch(method, url):
    return dashboard_sys.fetch(method, url)

def print(text):
    dashboard_sys.print(text)

def send(data: FrameData):
    dashboard_sys.send(data)
