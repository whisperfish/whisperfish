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
    property string peerName

    property var _type: modelData.messageType

    property string _outgoing: modelData.outgoing === true

    property bool _canShowDetails: (_type === "identity_reset" || _type === "session_reset") ? true : false
    property real _fontSize: Theme.fontSizeExtraSmall
    property url _iconSource: switch (_type) {
        case "expiration_timer_update":
            return "image://theme/icon-s-timer"
        case "missed_voice_call":
        case "missed_video_call":
            return "image://theme/icon-s-activity-missed-call"
        case "voice_call":
        case "video_call":
            return "image://theme/icon-s-activity-outgoing-call"
        case "identity_reset":
            return "image://theme/icon-s-outline-secure"
        case "session_reset":
            return "image://theme/icon-s-developer"
        case "group_change":
            return "image://theme/icon-s-sync"
        case "joined_group":
            return "image://theme/icon-s-new"
        case "left_group":
            return "image://theme/icon-s-blocked"
        default:
            return ""
    }

    function timeFormat(secs) {
        if (secs >= 604800 && secs % 604800 === 0)
            //: Expiring message timeout in weeks. Used in whisperfish-service-message-expiry-update-[self|peer]
            //% "%n week(s)"
            return qsTrId("whisperfish-service-message-expiry-in-weeks", Math.floor(secs / 604800))
        else if (secs >= 86400 && secs % 86400 === 0)
            //: Expiring message timeout in days. Used in whisperfish-service-message-expiry-update-[self|peer]
            //% "%n day(s)"
            return qsTrId("whisperfish-service-message-expiry-in-days", Math.floor(secs / 86400))
        else if (secs >= 3600 && secs % 3600 === 0)
            //: Expiring message timeout in hours. Used in whisperfish-service-message-expiry-update-[self|peer]
            //% "%n hour(s)"
            return qsTrId("whisperfish-service-message-expiry-in-hours", Math.floor(secs / 3600))
        else if (secs >= 60 && secs % 60 === 0)
            //: Expiring message timeout in minutes. Used in whisperfish-service-message-expiry-update-[self|peer]
            //% "%n minute(s)"
            return qsTrId("whisperfish-service-message-expiry-in-minutes", Math.floor(secs / 60))
        else
            //: Expiring message timeout in seconds. Used in whisperfish-service-message-expiry-update-[self|peer]
            //% "%n second(s)"
            return qsTrId("whisperfish-service-message-expiry-in-seconds", Math.floor(secs))
    }

    property string _message: switch (_type) {
        case "expiration_timer_update":
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
                ? qsTrId("whisperfish-service-message-expiry-update-self").arg(timeFormat(secs))
                //: Service message, %1 is a name, %2 is time
                //% "%1 set expiring messages timeout to %2."
                : qsTrId("whisperfish-service-message-expiry-update-peer").arg(peerName).arg(timeFormat(secs))
            } else if (secs === 0) {
                return _outgoing
                //: Service message
                //% "You disabled expiring messages."
                ? qsTrId("whisperfish-service-message-expiry-disable-self")
                //: Service message, %1 is a name
                //% "%1 disabled expiring messages."
                : qsTrId("whisperfish-service-message-expiry-disable-peer").arg(peerName)
            } else {
                return _outgoing
                //: Service message
                //% "You set or disabled expiring messages timeout."
                ? qsTrId("whisperfish-service-message-expiry-unknown-self")
                //: Service message, %1 is a name
                //% "%1 set or disabled expiring messages timeout."
                : qsTrId("whisperfish-service-message-expiry-unknown-peer").arg(peerName)
            }
        case "profile_key_update": // incoming only
            //: Service message for profile (key) update. %1 is a name
            //% "%1 updated their profile."
            return qsTrId("whisperfish-service-message-profile-key-update-peer").arg(peerName)
        case "end_session":
            return _outgoing
            //: Service message, %1 is a name
            //% "You ended the session with %1."
            ? qsTrId("whisperfish-service-message-end-session-self").arg(peerName)
            //: Service message, %1 is a name
            //% "%1 ended the session with you."
            : qsTrId("whisperfish-service-message-end-session-peer").arg(peerName)
        case "group_change":
            //: Service message
            //% "The group was updated."
            return qsTrId("whisperfish-service-message-changed-group")
        case "joined_group":
            return _outgoing
            //: Service message
            //% "You joined the group."
            ? qsTrId("whisperfish-service-message-joined-group-self")
            //: Service message, %1 is a name
            //% "%1 joined the group."
            : qsTrId("whisperfish-service-message-joined-group-peer").arg(peerName)
        case "left_group":
            return _outgoing
            //: Service message, %1 is a name
            //% "You left the group."
            ? qsTrId("whisperfish-service-message-left-group-self")
            //: Service message, %1 is a name
            //% "%1 left the group."
            : qsTrId("whisperfish-service-message-left-group-peer").arg(peerName)
        case "missed_voice_call":
            return _outgoing
            //: Service message, %1 is a name
            //% "You missed a voice call from %1."
            ? qsTrId("whisperfish-service-message-missed-call-voice-self").arg(peerName)
            //: Service message, %1 is a name
            //% "You tried to voice call %1."
            : qsTrId("whisperfish-service-message-missed-call-voice-peer").arg(peerName)
        case "missed_video_call":
            return _outgoing
            //: Service message, %1 is a name
            //% "You missed a video call from %1."
            ? qsTrId("whisperfish-service-message-missed-call-video-self").arg(peerName)
            //: Service message, %1 is a name
            //% "You tried to video call %1."
            : qsTrId("whisperfish-service-message-missed-call-video-peer").arg(peerName)
        case "video_call":
            return _outgoing
            //: Service message, %1 is a name
            //% "You had a video call with %1."
            ? qsTrId("whisperfish-service-message-call-video-self").arg(peerName)
            //: Service message, %1 is a name
            //% "%1 had a video call with you."
            : qsTrId("whisperfish-service-message-call-video-peer").arg(peerName)
        case "voice_call":
            return _outgoing
            //: Service message, %1 is a name
            //% "You had a voice call with %1."
            ? qsTrId("whisperfish-service-message-call-voice-self").arg(peerName)
            //: Service message, %1 is a name
            //% "%1 had a voice call with you."
            : qsTrId("whisperfish-service-message-call-voice-peer").arg(peerName)
        case "identity_reset":
            //: Service message, %1 is a name
            //% "Your safety number with %1 has changed. "
            //% "Swipe right to verify the new number."
            return qsTrId("whisperfish-service-message-fingerprint-changed").arg(peerName)
        case "session_reset":
            return _outgoing
            //: Service message, %1 is a name
            //% "You reset the secure session with %1."
            ? qsTrId("whisperfish-service-message-session-reset-self").arg(peerName)
            //: Service message, %1 is a name
            //% "%1 reset the secure session with you."
            : qsTrId("whisperfish-service-message-session-reset-peer").arg(peerName)
        default:
            //: Service message, %1 is an integer, %2 is a word, %3 is the message text (if any)
            console.warn("Unsupported service message: id", modelData.id, "flags", modelData.flags, "type", _type, "text", modelData.message)
            //% "This service message of is not yet supported by Whisperfish. "
            //% "Please file a bug report. (Flags: %1, Type: %2, Contents: \"%3\")"
            return qsTrId("whisperfish-service-message-not-supported").arg(modelData.flags).arg(_type).arg(modelData.message != null ? modelData.message : "NULL")
    }

    function showDetails() {
        var locale = Qt.locale().name.replace(/_.*$/, '').toLowerCase()
        if (!/[a-z][a-z]/.test(locale)) locale = "en-us"

        if (_type === "fingerprint_changed") {
            // "What is a safety number and why do I see that it changed?"
            Qt.openUrlExternally('https://support.signal.org/hc/%1/articles/360007060632'.arg(locale))
        } else if (_type === "session_reset") {
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
            text: (SettingsBridge.debug_mode ? "[" + modelData.id + "] " : "") + _message
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
