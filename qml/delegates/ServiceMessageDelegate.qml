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
    property QtObject recipient

    property var _type: modelData != null ? modelData.messageType : null

    property string _outgoing: modelData.outgoing === true
    property string _originName: (modelData !== null) && modelData.recipientId > 0 ? getRecipientName(recipient.e164, recipient.name, false) : ''

    property bool _canShowDetails: (_type === "IdentityKeyChange" || _type === "SessionReset") ? true : false
    property int _fontSize: Theme.fontSizeExtraSmall
    property url _iconSource: switch (_type) {
        case "ExpirationTimerUpdate":
            return "image://theme/icon-s-timer"
        case "MissedVoiceCall":
        case "MissedCallVideo":
            return "image://theme/icon-s-activity-missed-call"
        case "VoiceCall":
        case "CallVideo":
            return "image://theme/icon-s-activity-outgoing-call"
        case "IdentityKeyChange":
            return "image://theme/icon-s-outline-secure"
        case "SessionReset":
            return "image://theme/icon-s-developer"
        case "GroupChange":
            return "image://theme/icon-s-sync"
        case "JoinedGroup":
            return "image://theme/icon-s-new"
        case "LeftGroup":
            return "image://theme/icon-s-blocked"
        default:
            return ""
    }

    function timeFormat(secs) {
        // These translations are defined in ExpiringMessagesComboBox.qml
        if (secs >= 604800 && secs % 604800 === 0)
            return qsTrId("whisperfish-disappearing-messages-weeks", Math.floor(secs / 604800))
        else if (secs >= 86400 && secs % 86400 === 0)
            return qsTrId("whisperfish-disappearing-messages-days", Math.floor(secs / 86400))
        else if (secs >= 3600 && secs % 3600 === 0)
            return qsTrId("whisperfish-disappearing-messages-hours", Math.floor(secs / 3600))
        else if (secs >= 60 && secs % 60 === 0)
            return qsTrId("whisperfish-disappearing-messages-minutes", Math.floor(secs / 60))
        else
            return qsTrId("whisperfish-disappearing-messages-seconds", Math.floor(secs))
    }

    property string _message: switch (_type) {
        case "ExpirationTimerUpdate":
            // We didn't save the expiresIn for the service messages themselves,
            // so we may have to parse the "placeholder" text for the value instead.
            var secs = modelData.expiresIn
            if (secs == -1 && modelData.message.length > 0) {
                var matches = modelData.message.match(/(\d)+/)
                if (Array.isArray(matches)) {
                    secs = matches[0]
                } else if (modelData.message.includes("None")) {
                    secs = 0
                }
            }

            if (secs > 0) {
                return _outgoing
                //: Service message, %1 time
                //% "You set expiring messages timeout to %1."
                ? qsTrId("whisperfish-service-message-expity-update-self").arg(timeFormat(secs))
                //: Service message, %1 is a name, %2 is time
                //% "%1 set expiring messages timeout to %2."
                : qsTrId("whisperfish-service-message-expity-update-peer").arg(_originName).arg(timeFormat(secs))
            } else if (secs === 0) {
                return _outgoing
                //: Service message
                //% "You disabled expiring messages."
                ? qsTrId("whisperfish-service-message-expity-disable-self")
                //: Service message, %1 is a name
                //% "%1 disabled expiring messages."
                : qsTrId("whisperfish-service-message-expity-disable-peer").arg(_originName)
            } else {
                return _outgoing
                //: Service message
                //% "You set or disabled expiring messages timeout."
                ? qsTrId("whisperfish-service-message-expity-unknown-self")
                //: Service message, %1 is a name
                //% "%1 set or disabled expiring messages timeout."
                : qsTrId("whisperfish-service-message-expity-unknown-peer").arg(_originName)
            }
        case "ProfileKeyUpdate":
            return _outgoing
            //: Service message, %1 is a name
            //% "You updated your profile key with %1."
            ? qsTrId("whisperfish-service-message-profile-key-update-self").arg(_originName)
            //: Service message, %1 is a name, %2 is time
            //% "%1 updated their profile key with you."
            : qsTrId("whisperfish-service-message-profile-key-update-peer").arg(_originName)
        case "EndSession":
            return _outgoing
            //: Service message, %1 is a name
            //% "You ended the session with %1."
            ? qsTrId("whisperfish-service-message-end-session-self").arg(_originName)
            //: Service message, %1 is a name
            //% "%1 ended the session with you."
            : qsTrId("whisperfish-service-message-end-session-peer").arg(_originName)
        case "GroupChange":
            return _outgoing
            //: Service message
            //% "You changed something in the group."
            ? qsTrId("whisperfish-service-message-changed-group-self")
            //: Service message, %1 is a name
            //% "%1 changed something in the group."
            : qsTrId("whisperfish-service-message-changed--group-peer").arg(_originName)
        case "JoinedGroup":
            return _outgoing
            //: Service message
            //% "You joined the group."
            ? qsTrId("whisperfish-service-message-joined-group-self")
            //: Service message, %1 is a name
            //% "%1 joined the group."
            : qsTrId("whisperfish-service-message-joined-group-peer").arg(_originName)
        case "LeftGroup":
            return _outgoing
            //: Service message, %1 is a name
            //% "You left the group."
            ? qsTrId("whisperfish-service-message-left-group-self")
            //: Service message, %1 is a name
            //% "%1 left the group."
            : qsTrId("whisperfish-service-message-left-group-peer").arg(_originName)
        case "MissedVoiceCall":
            return _outgoing
            //: Service message, %1 is a name
            //% "You missed a voice call from %1."
            ? qsTrId("whisperfish-service-message-missed-call-voice-self").arg(_originName)
            //: Service message, %1 is a name
            //% "You tried to voice call %1."
            : qsTrId("whisperfish-service-message-missed-call-voice-peer").arg(_originName)
        case "MissedVideoCall":
            return _outgoing
            //: Service message, %1 is a name
            //% "You missed a video call from %1."
            ? qsTrId("whisperfish-service-message-missed-call-video-self").arg(_originName)
            //: Service message, %1 is a name
            //% "You tried to video call %1."
            : qsTrId("whisperfish-service-message-missed-call-video-peer").arg(_originName)
        case "VideoCall":
            return _outgoing
            //: Service message, %1 is a name
            //% "You had a video call with %1."
            ? qsTrId("whisperfish-service-message-call-video-self").arg(_originName)
            //: Service message, %1 is a name
            //% "%1 had a video call with you."
            : qsTrId("whisperfish-service-message-call-video-peer").arg(_originName)
        case "VoiceCall":
            return _outgoing
            //: Service message, %1 is a name
            //% "You had a voice call with %1."
            ? qsTrId("whisperfish-service-message-call-voice-self").arg(_originName)
            //: Service message, %1 is a name
            //% "%1 had a voice call with you."
            : qsTrId("whisperfish-service-message-call-voice-peer").arg(_originName)
        case "IdentityKeyChange":
            //: Service message, %1 is a name
            //% "Your safety number with %1 has changed. "
            //% "Swipe right to verify the new number."
            qsTrId("whisperfish-service-message-fingerprint-changed").arg(_originName)
        case "SessionReset":
            return _outgoing
            //: Service message, %1 is a name
            //% "You reset the secure session with %1."
            ? qsTrId("whisperfish-service-message-session-reset-self").arg(_originName)
            //: Service message, %1 is a name
            //% "%1 reset the secure session with you."
            : qsTrId("whisperfish-service-message-session-reset-peer").arg(_originName)
        default:
            //: Service message, %1 is an integer
            console.warn("Unsupported service message: id", modelData.id, "flags", modelData.flags, "text", modelData.message)
            //% "This service message is not yet supported by Whisperfish. "
            //% "Please file a bug report. (Type: %1)"
            return qsTrId("whisperfish-service-message-not-supported").arg(modelData.flags)
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
