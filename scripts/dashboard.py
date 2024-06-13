import dashboard_sys
import json
from dataclasses import dataclass
import sys


@dataclass
class FrameData:
    action: str
    name: str
    value: str
    id: str


def fetch(method, url):
    return dashboard_sys.fetch(method, url)


def print(text):
    dashboard_sys.print(str(text))


def send(id: str, action: str, name: str, value: dict):
    dashboard_sys.send(
        FrameData(id=id, action=action, name=name, value=json.dumps(value, ensure_ascii=False))
    )


def image(id: str, name: str, filepath: str):
    send(
        id=id,
        action="image",
        name=name,
        value=dict(
            filepath=filepath,
        ),
    )


def text(id: str, name: str, text: str, *, color: str = "white", align="center"):
    send(id=id, action="text", name=name, value=dict(text=text, color=color, align=align))


def styled_text(id: str, name: str, text: list[dict], *, color: str = "white", align: str = "center"):
    send(id=id, action="color_text", name=name, value=dict(color=color, lines=text, align=align))


def make_text(
    text: str,
    *,
    color: str = "white",
    bold: bool = False,
    underline: bool = False,
    italic: bool = False,
    crossline: bool = False,
):
    return dict(
        text=text,
        color=color,
        bold=bold,
        underline=underline,
        italic=italic,
        crossline=crossline,
    )

def chart(
    id: str,
    name: str,
    data: list[(float, float)],
    *,
    description: str = "",
    graph_type: str = "line",
    marker_type: str = "braille",
    color: str = "white",
    x_title: str = "",
    x_color: str = "blue",
    x_bounds: tuple[float, float] = (0., 10.),
    x_labels: list[str] = [],
    y_title: str = "",
    y_color: str = "red",
    y_bounds: tuple[float, float] = (0., 10.),
    y_labels: list[str] = [],
):
    send(id=id, action="chart", name=name, value=dict(
        data=data,
        name=name,
        description=description,
        graph_type=graph_type,
        marker_type=marker_type,
        color=color,
        x_title=x_title,
        x_color=x_color,
        x_bounds=x_bounds,
        x_labels=x_labels,
        y_title=y_title,
        y_color=y_color,
        y_bounds=y_bounds,
        y_labels=y_labels,
    ))

def big_text(id: str, name: str, text: str, *, color: str = "white", align: str = "center"):
    send(id=id, action="big", name=name, value=dict(text=text, color=color, align=align))


def reload_scripts():
    send(id="reload", action="reload", name="reload", value=dict())


def todo_add(id: str, text: str, by: str, deadline: int):
    send(id=id, action="todo_add", name=id, value=dict(text=text, by=by, deadline=deadline))

def todo_done(index: int):
    send(id="todo", action="todo_done", name="", value=dict(index=index))


def todo_del(index: int):
    send(id="todo", action="todo_del", name="", value=dict(index=index))


def exit():
    send(id="exit", action="exit", name="", value=dict())


class FileOut(object):
    def write(self, text):
        global print
        print(text)


sys.stdout = FileOut()
sys.stderr = FileOut()
