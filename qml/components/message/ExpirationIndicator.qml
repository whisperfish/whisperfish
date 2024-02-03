// SPDX-FileCopyrightText: 2023 Matti Viljanen
// SPDX-License-Identifier: AGPL-3.0-or-later
import QtQuick 2.5

Canvas {
    property real expiresIn: -1 // in seconds
    property var expiryStarted: undefined // Date()
    property var color: "#ffffff"

    property var running: true // for external timer

    property bool _shouldPaint: Qt.application.active && expiresIn != -1

    property real _now               // current timestamp in milliseconds
    property real _expiryStarted: -1 // expiry started timestamp in milliseconds (defaults to _now)
    property real _expires           // expiration timestamp in milliseconds
    property real _duration          // expiration period in milliseconds
    property bool _cleared: false    // is the canvas cleared or "dirty"
    property bool _isExpiring: true  // is the expiration in progress
    property real _endAngle          // the end angle of the "pie chart" (0...2*pi)
    property real _prevAngle: 7.0    // what the previous angle was to reduce frequent drawing (initially >2*pi)

    renderStrategy: Canvas.Threaded
    renderTarget: Canvas.FramebufferObject
    onPaint: {
        if (!_shouldPaint) return

        var ctx = getContext("2d")

        // Origin to the center, 0 degrees is up
        ctx.setTransform(1, 0, 0, 1, width/2, width/2)

        // Get timestamps in milliseconds
        _now = (new Date()).valueOf()
        if (_expiryStarted == -1) {
            if (expiryStarted == null) {
                _expiryStarted = _now
            } else {
                _expiryStarted = expiryStarted.valueOf()
            }
        }
        _expires = expiresIn * 1000 + _expiryStarted
        _duration = _expires - _now
        // If expiration is in the past, we're done
        if (_duration < 0) {
            _shouldPaint = false
            running = false
            ctx.clearRect(-width/2,-width/2,width,width)
            return
        }

        _endAngle = Math.min(Math.max(0, _duration), (expiresIn * 1000)) / (expiresIn * 1000) * Math.PI*2
        if(_prevAngle - _endAngle < 0.1) { // ~6° or one second's worth
            return
        }
        _prevAngle = _endAngle

        ctx.clearRect(-width/2,-width/2,width,width)
        ctx.rotate(-Math.PI/2)

        ctx.beginPath()

        ctx.fillStyle = color
        ctx.strokeStyle = color
        ctx.lineWidth = 1;

        ctx.moveTo(0, 0)
        ctx.arc(0, 0, width / 3, 0, _endAngle, false)
        ctx.moveTo(0, 0)

        ctx.fill()
        ctx.stroke()
        _cleared = false
    }
}
