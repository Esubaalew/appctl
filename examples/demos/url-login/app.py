"""Tiny Flask app with session login for `appctl sync --url` experiments."""
from flask import Flask, redirect, render_template_string, request, session

app = Flask(__name__)
app.secret_key = "demo-secret-change-me"

LOGIN = """
<form method="post">
  <input name="user" placeholder="user" />
  <input name="password" type="password" placeholder="password" />
  <button type="submit">Login</button>
</form>
"""

HOME = "<p>ok {{ session.user }}</p><a href=/logout>logout</a>"


@app.route("/login", methods=["GET", "POST"])
def login():
    if request.method == "POST":
        session["user"] = request.form.get("user", "")
        return redirect("/")
    return render_template_string(LOGIN)


@app.route("/logout")
def logout():
    session.clear()
    return redirect("/login")


@app.route("/")
def home():
    if not session.get("user"):
        return redirect("/login")
    return render_template_string(HOME)


if __name__ == "__main__":
    app.run(host="127.0.0.1", port=5009)
