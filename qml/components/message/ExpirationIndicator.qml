// SPDX-FileCopyrightText: 2023 Matti Viljanen
// SPDX-License-Identifier: AGPL-3.0-or-later
import QtQuick 2.5
import Sailfish.Silica 1.0

Rectangle {
    id: root

    property real expiresIn: -1 // in seconds
    readonly property int expiresInMs: expiresIn > 0 ? expiresIn * 1000 : 0
    readonly property bool isRunning: root.expiresIn > 0 && expiryStarted !== undefined
    property var expiryStarted: undefined // Date()
    property alias itemColor: expiryBar.color

    readonly property real unit: height / 14

    color: "transparent"
    border.width: unit
    border.color: expiryBar.color

    property real barHeight: progress * 10 * unit
    property real progress: 0.0
    radius: unit * 2

    Component.onCompleted: update()
    onExpiresInChanged: update()
    onExpiryStartedChanged: update()

    Rectangle {
        id: expiryBar
        anchors.bottom: parent.bottom
        anchors.bottomMargin: 2 * unit
        anchors.horizontalCenter: parent.horizontalCenter
        width: parent.width - 4 * unit
        height: barHeight
    }

    function update() {
        if (expiresIn < 0 || !expiryStarted) {
            progress = 0.0
            return
        }

        var now = (new Date()).valueOf()
        var remainingMs = (expiryStarted.valueOf() + expiresInMs) - now

        if (remainingMs <= 0) {
            progress = 0.0
            return
        }

        var next = remainingMs / expiresInMs
        if (next > 1.0) {
            next = 1.0
        }
        if (next <= 0.0) {
            next = 0.0
        }
        progress = next
    }

    Timer {
        id: timer
        running: root.isRunning
        repeat: true
        interval: Math.max(500, expiresInMs / 25)
        onTriggered: {
            update()
            console.log("update", modelData.id)
        }
    }
}
