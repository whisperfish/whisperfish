// SPDX-FileCopyrightText: 2021 Mirian Margiani
// SPDX-License-Identifier: AGPL-3.0-or-later

import QtQuick 2.6
import Sailfish.Silica 1.0
// import "../components"

ListItem {
    id: delegate
    contentHeight: column.height
    width: parent.width
    enabled: _canShowDetails
    onClicked: showDetails()

    property QtObject modelData

    // TODO the model should expose the message type as enum
    // TODO what service messages are there?
    // TODO do we need special treatment for service messages in groups?
    property string _origin: "none" /* modelData.serviceMessageOrigin */ // "self" or "peer"
    property string _type: switch (modelData.flags) {
            // DataMessage::Flags
            case 1:   /* 1 << 0 */ return "endSession"
            case 2:   /* 1 << 1 */ return "expiryUpdate"
            case 4:   /* 1 << 2 */ return "profileKeyUpdate"

            // Whisperfish-only values:
            case 256: /* 1 << 8 */ return "identityKeyUpdate"

            default: return ""
        }

    property string _originName: (modelData !== null) ? getRecipientName(modelData.recipientE164, modelData.recipientName, false) : ''

    property bool _canShowDetails: (_type === "fingerprintChanged" /*||
                                    _type === "sessionReset"*/) ?
                                       true : false
    property int _fontSize: Theme.fontSizeExtraSmall
    property url _iconSource: {
        if (_type === "missedCallVoice" || _type === "missedCallVideo") {
            "image://theme/icon-s-activity-missed-call"
        } else if (_type === "callVoice" || _type === "callVideo") {
            "image://theme/icon-s-activity-outgoing-call"
        } else if (_type === "fingerprintChanged") {
            "image://theme/icon-s-outline-secure"
        } else if (_type === "sessionReset") {
            "image://theme/icon-s-checkmark"
        } else if (_type === "joinedGroup" || _type === "leftGroup") {
            "image://theme/icon-m-outline-chat" // TODO we need a small outline icon here
        } else {
            ""
        }
    }
    property string _message: {
        if (_type === "joinedGroup" && _origin === "self") {
            //: Service message, %1 = group name
            //% "You joined the group “%1”."
            qsTrId("whisperfish-service-message-joined-group-self").arg(_originName)
        } else if (_type === "leftGroup" && _origin === "self") {
            //: Service message
            //% "You left the group."
            qsTrId("whisperfish-service-message-left-group-self")
        } else if (_type === "joinedGroup" && _origin === "peer") {
            //: Service message, %1 is the new member's name
            //% "%1 joined the group."
            qsTrId("whisperfish-service-message-joined-group-peer").arg(_originName)
        } else if (_type === "leftGroup" && _origin === "peer") {
            //: Service message, %1 is is the lost member's name
            //% "%1 left the group."
            qsTrId("whisperfish-service-message-left-group-peer").arg(_originName)
        } else if (_type === "missedCallVoice") {
            //: Service message, %1 is a name
            //% "You missed a call from %1."
            qsTrId("whisperfish-service-message-missed-call-voice").arg(_originName)
        } else if (_type === "missedCallVideo") {
            //: Service message, %1 is a name
            //% "You missed a video call from %1."
            qsTrId("whisperfish-service-message-missed-call-video").arg(_originName)
        } else if (_type === "callVoice" && _origin === "self") {
            //: Service message, %1 is a name
            //% "You called %1."
            qsTrId("whisperfish-service-message-call-voice-self").arg(_originName)
        } else if (_type === "callVideo" && _origin === "self") {
            //: Service message, %1 is a name
            //% "You started a video call with %1."
            qsTrId("whisperfish-service-message-call-video-self").arg(_originName)
        } else if (_type === "callVoice" && _origin === "peer") {
            //: Service message, %1 is a name
            //% "%1 called you."
            qsTrId("whisperfish-service-message-call-voice-peer").arg(_originName)
        } else if (_type === "callVideo" && _origin === "peer") {
            //: Service message, %1 is a name
            //% "%1 started a video call with you."
            qsTrId("whisperfish-service-message-call-video-peer").arg(_originName)
        } else if (_type === "fingerprintChanged") {
            //: Service message, %1 is a name
            //% "Your safety number with %1 has changed. "
            //% "Swipe right to verify the new number."
            qsTrId("whisperfish-service-message-fingerprint-changed").arg(_originName)
        } else if (_type === "sessionReset" && _origin === "self") {
            //: Service message, %1 is a name
            //% "You have reset the secure session with %1."
            qsTrId("whisperfish-service-message-session-reset-self").arg(_originName)
        } else if (_type === "sessionReset" && _origin === "peer") {
            //: Service message, %1 is a name
            //% "%1 has reset the secure session with you."
            qsTrId("whisperfish-service-message-session-reset-peer").arg(_originName)
        } else {
            //: Service message, %1 is an internal message type identifier
            //% "This service message is not yet supported by Whisperfish. "
            //% "Please file a bug report. (Type: '%1'.)"
            qsTrId("whisperfish-service-message-not-supported").arg(_type)
        }
    }

    function showDetails() {
        var locale = Qt.locale().name.replace(/_.*$/, '').toLowerCase()
        if (!/[a-z][a-z]/.test(locale)) locale = "en-us"

        if (_type === "fingerprintChanged") {
            // "What is a safety number and why do I see that it changed?"
            Qt.openUrlExternally('https://support.signal.org/hc/%1/articles/360007060632'.arg(locale))
        } else if (_type === "sessionReset") {
            // there seems to be no help article on the issue
            // Qt.openUrlExternally("")
        } else {
            console.warn("cannot show details for service message type:", _type)
            console.log("check and compare _canShowDetails and showDetails()")
        }
    }

    Column {
        id: column
        anchors.horizontalCenter: parent.horizontalCenter
        width: parent.width - 4*Theme.horizontalPageMargin
        spacing: Theme.paddingSmall
        topPadding: Theme.paddingMedium
        bottomPadding: Theme.paddingMedium

        HighlightImage {
            // We show the icon in a separate HighlightImage item
            // because Labels don't support coloring icons and 'image://'
            // urls as source.
            // (Otherwise we could include the icon inline in the label
            // by setting 'textFormat: Text.StyledText' and using
            // '<img src="%1" align="middle" width="%2" height="%2">'.)
            anchors.horizontalCenter: parent.horizontalCenter
            width: source !== "" ? Theme.iconSizeSmall : 0
            height: width
            color: Theme.secondaryHighlightColor
            source: _iconSource
        }

        Label {
            width: parent.width
            horizontalAlignment: Text.AlignHCenter
            wrapMode: Text.Wrap
            text: _message
            color: Theme.secondaryHighlightColor
            font.pixelSize: _fontSize
            textFormat: Text.PlainText
        }

        Label {
            visible: _canShowDetails
            width: parent.width
            horizontalAlignment: Text.AlignHCenter
            //% "more information"
            text: "<a href='#'>"+qsTrId("whisperfish-service-message-more-info")+"</a>"
            textFormat: Text.StyledText
            onLinkActivated: showDetails()
            color: Theme.secondaryColor
            linkColor: color
            font.pixelSize: _fontSize
        }
    }
}
