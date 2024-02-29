/****************************************************************************************
**
** Copyright (C) 2013 Jolla Ltd.
** Contact: Joona Petrell <joona.petrell@jollamobile.com>
** All rights reserved.
**
** This file is part of Sailfish Silica UI component package.
**
** You may use this file under the terms of BSD license as follows:
**
** Redistribution and use in source and binary forms, with or without
** modification, are permitted provided that the following conditions are met:
**     * Redistributions of source code must retain the above copyright
**       notice, this list of conditions and the following disclaimer.
**     * Redistributions in binary form must reproduce the above copyright
**       notice, this list of conditions and the following disclaimer in the
**       documentation and/or other materials provided with the distribution.
**     * Neither the name of the Jolla Ltd nor the
**       names of its contributors may be used to endorse or promote products
**       derived from this software without specific prior written permission.
**
** THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS" AND
** ANY EXPRESS OR IMPLIED WARRANTIES, INCLUDING, BUT NOT LIMITED TO, THE IMPLIED
** WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE
** DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDERS OR CONTRIBUTORS BE LIABLE FOR
** ANY DIRECT, INDIRECT, INCIDENTAL, SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES
** (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR SERVICES;
** LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND
** ON ANY THEORY OF LIABILITY, WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT
** (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE USE OF THIS
** SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.
**
****************************************************************************************/

// Adapted for Whisperfish:
// SPDX-FileCopyrightText: 2021 Mirian Margiani
// SPDX-License-Identifier: AGPL-3.0-or-later
//
// We use our own page header to fix a bug in the original
// PageHeader which let long page titles extend over the extra content.
// Once the backend supports it, we can include profile pictures
// and other eye candy here.

import QtQuick 2.6
import Sailfish.Silica 1.0
// import "private/Util.js" as Util

SilicaItem {
    id: pageHeader

    property bool isGroup: false
    property string profilePicture: ''

    property alias title: headerText.text
    property string description: ''
    property int descriptionWrapMode: Text.NoWrap

    property alias _titleItem: headerText
    property alias wrapMode: headerText.wrapMode
    property Item page
    property alias titleColor: headerText.color
    property real leftMargin: 1.5*Theme.horizontalPageMargin
    property real rightMargin: Theme.horizontalPageMargin
    property real _preferredHeight: page && page.isLandscape ? Theme.itemSizeSmall : Theme.itemSizeLarge
    property string isTypingMessage: ""

    onIsTypingMessageChanged: if (isTypingMessage.length > 0) {
        clearTypingTimer.restart()
    }

    Component.onCompleted: {
        if (!page) {
            // page = Util.findPage(pageHeader)
            page = pageStack.currentPage
        }
    }

    Connections {
        target: SessionModel
        onSendTypings: {
            // Only ConversationPage.qml has `sessionId` property.
            if(pageStack.currentPage.sessionId != typing_data.sid) {
                return
            }

            var count = typing_data.names.length
            if (count == 1)
                //: Text shown when one person is typing
                //% "%1 is typing"
                isTypingMessage = qsTrId("whisperfish-typing-1").arg(typing_data.names[0])
            else if (count == 2)
                //: Text shown when two persons are typing
                //% "%1 and %2 are typing"
                isTypingMessage = qsTrId("whisperfish-typing-2").arg(typing_data.names[0]).arg(typing_data.names[1])
            else if (count >= 3)
                //: Text shown when three or more persons are typing
                //% "%1 and %n others are typing"
                typing = qsTrId("whisperfish-typing-3-plus").arg(typing_data.names[0]).arg(count - 1)
            else
                isTypingMessage = ""
        }
    }

    Timer {
        id: clearTypingTimer
        running: false
        interval: 5000
        repeat: false
        onTriggered: isTypingMessage = ""
    }

    width: parent ? parent.width : Screen.width
    // set height that keeps the first line of text aligned with the page indicator
    height: Math.max(headerText.y + headerText.height + _isTypingLabel.height + Theme.paddingMedium,
                     _preferredHeight)

    Label {
        id: headerText
        // Don't allow the label to extend over the page stack indicator.
        // We cannot use anchors and horizontalAlignment: Text.AlignRight
        // because we want the truncation to happen at the right side.
        width: Math.min(implicitWidth, parent.width - extraContent.width -
                        Theme.paddingMedium - leftMargin - rightMargin)
        truncationMode: TruncationMode.Fade
        color: highlighted ? Theme.primaryColor : Theme.highlightColor
        // align first line with page indicator
        y: Math.floor(_preferredHeight/2 - metrics.height/2)
        anchors { right: parent.right; rightMargin: pageHeader.rightMargin }
        font {
            pixelSize: Theme.fontSizeLarge
            family: Theme.fontFamilyHeading
        }
        TextMetrics {
            id: metrics
            font: headerText.font
            text: "X"
        }
    }

    // _descriptionLabel and _isTypingLabel are rendered at the same position.
    // If _isTypingLabel has text, only it will be visible.
    // If not, only _descriptionLabel will be visible - empty or not.
    // No more visual elements bouncing around when someone is typing!

    Label {
        id: _descriptionLabel
        opacity: _isTypingLabel.text.length > 0 ? 0.0 : 1.0
        Behavior on opacity {
            NumberAnimation {}
        }
        clip: true
        anchors {
            top: _titleItem.bottom
            right: parent.right;
            rightMargin: parent.rightMargin
            left: extraContent.right;
            leftMargin: Theme.paddingMedium
        }
        font.pixelSize: Theme.fontSizeSmall
        color: highlighted ? Theme.secondaryColor : Theme.secondaryHighlightColor
        horizontalAlignment: wrapMode === Text.NoWrap && implicitWidth > width ?
                                 Text.AlignLeft : Text.AlignRight
        truncationMode: TruncationMode.Fade
        text: pageHeader.description
        wrapMode: pageHeader.wrapMode
    }

    Label {
        id: _isTypingLabel
        opacity: text.length > 0 ? 1.0 : 0.0
        Behavior on opacity {
            NumberAnimation {}
        }
        clip: true
        anchors {
            top: _titleItem.bottom
            right: parent.right;
            rightMargin: parent.rightMargin
            left: extraContent.right;
            leftMargin: Theme.paddingMedium
        }
        font.pixelSize: Theme.fontSizeSmall
        color: highlighted ? Theme.secondaryColor : Theme.secondaryHighlightColor
        horizontalAlignment: wrapMode === Text.NoWrap && implicitWidth > width ?
                                 Text.AlignLeft : Text.AlignRight
        truncationMode: TruncationMode.Fade
        text: pageHeader.isTypingMessage
        wrapMode: pageHeader.wrapMode
    }

    ProfilePicture {
        id: extraContent
        highlighted: false
        labelsHighlighted: false
        imageSource: profilePicture
        isGroup: pageHeader.isGroup
        showInfoMark: false
        anchors {
            verticalCenter: parent.verticalCenter
            left: parent.left; leftMargin: pageHeader.leftMargin
        }
        onClicked: pageStack.navigateForward(PageStackAction.Animated)
    }
}
