"""
py2app build script for People

Usage:
    ./build

Output will be in dist/ directory
"""

import os
from setuptools import setup


def tree(src):
    """Recursively collect files from a directory."""
    return [
        (root, [os.path.join(root, f) for f in files])
        for root, dirs, files in os.walk(src)
    ]


APP = ["src/main.py"]
APP_NAME = "People"

DATA_FILES = [
    ("templates", ["src/templates/index.html"]),
    ("static", [
        "src/static/aqua.css",
        "src/static/bg.jpg",
        "src/static/icon.png",
        "src/static/layout.css",
        "src/static/reset.css",
    ]),
    ("static/fonts", [
        "src/static/fonts/HedvigLettersSerif.ttf",
        "src/static/fonts/InclusiveSans-Regular.ttf",
        "src/static/fonts/InclusiveSans-Bold.ttf",
        "src/static/fonts/InclusiveSans-Italic.ttf",
        "src/static/fonts/InclusiveSans-BoldItalic.ttf",
    ]),
]

OPTIONS = {
    "argv_emulation": False,
    "strip": True,
    "iconfile": "icon/People.icns",
    "includes": [
        "flask",
        "jinja2",
        "markupsafe",
        "werkzeug",
        "click",
        "itsdangerous",
        "blinker",
    ],
    "packages": [
        "flask",
        "jinja2",
        "Contacts",
    ],
    "excludes": [
        "setuptools",
        "pkg_resources",
    ],
    "frameworks": [],
    "plist": {
        "CFBundleName": APP_NAME,
        "CFBundleDisplayName": APP_NAME,
        "CFBundleIdentifier": "com.people.app",
        "CFBundleVersion": "1.0.0",
        "CFBundleShortVersionString": "1.0.0",
        "NSHighResolutionCapable": True,
        "NSAppleEventsUsageDescription": "People needs to send messages via Messages.app",
        "NSContactsUsageDescription": "People needs access to contacts to display names",
    },
    "bdist_base": ".build",
    "dist_dir": "out",
}

setup(
    name=APP_NAME,
    app=APP,
    data_files=DATA_FILES,
    options={"py2app": OPTIONS},
    setup_requires=["py2app"],
)
