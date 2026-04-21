"""Minimal FastAPI app — OpenAPI is generated automatically."""
from fastapi import FastAPI
from pydantic import BaseModel

app = FastAPI(title="appctl demo API", version="1.0.0")


class WidgetIn(BaseModel):
    name: str


@app.post("/widgets", status_code=201)
def create_widget(body: WidgetIn):
    return {"id": 1, "name": body.name}
