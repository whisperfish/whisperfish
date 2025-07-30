// SPDX-FileCopyrightText: 2021 Mirian Margiani
// SPDX-License-Identifier: AGPL-3.0-or-later

import QtQuick 2.6
import Sailfish.Silica 1.0
import be.rubdos.whisperfish 1.0
// import "../components"

ListItem {
    id: delegate
    contentHeight: column.height
    width: parent.width
    enabled: _canShowDetails
    onClicked: showDetails()

    property QtObject modelData
    property int recipientId // the individual message sender, "editor"
    // TODO: Don't query Recipient for every service message
    //       (Then again, the service messages are few and far between...)
    property QtObject recipient: Recipient {
        app: AppState
        recipientId: modelData.senderRecipientId
    }
    property string peerName: recipient.valid ? getRecipientName(recipient.e164, recipient.externalId, recipient.name, true) : ""

    property var _type: modelData.messageType

    property string _outgoing: modelData.outgoing === true

    property var _json: _type == "group_change" && modelData.message && modelData.message[0] === "{" ? JSON.parse(modelData.message) : undefined
    property var _data: (_type == "group_change" && _json) ? _json : null

    property bool _canShowDetails: (_type === "identity_reset" || _type === "session_reset") ? true : false
    property real _fontSize: Theme.fontSizeExtraSmall
    property url _iconSource: switch (_type) {
        case "expiration_timer_update":
            return "image://theme/icon-s-timer"
        case "missed_audio_call":
        case "missed_video_call":
            return "image://theme/icon-s-activity-missed-call"
        case "group_call":
            // XXX m-size is not ideal
            return "image://theme/icon-m-call"
        case "incoming_audio_call":
        case "incoming_video_call":
            return "image://theme/icon-s-activity-incoming-call"
        case "outgoing_audio_call":
        case "outgoing_video_call":
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

    function expiryMessage(outgoing, rcptName, seconds) {
        if (seconds > 0) {
            return outgoing
            //: Service message, %1 time
            //% "You set expiring messages timeout to %1."
            ? qsTrId("whisperfish-service-message-expiry-update-self").arg(timeFormat(seconds))
            //: Service message, %1 is a name, %2 is time
            //% "%1 set expiring messages timeout to %2."
            : qsTrId("whisperfish-service-message-expiry-update-peer").arg(rcptName).arg(timeFormat(seconds))
        } else if (_data.value === 0) {
            return outgoing
            //: Service message
            //% "You disabled expiring messages."
            ? qsTrId("whisperfish-service-message-expiry-disable-self")
            //: Service message, %1 is a name
            //% "%1 disabled expiring messages."
            : qsTrId("whisperfish-service-message-expiry-disable-peer").arg(rcptName)
        } else {
            return outgoing
            //: Service message
            //% "You set or disabled expiring messages timeout."
            ? qsTrId("whisperfish-service-message-expiry-unknown-self")
            //: Service message, %1 is a name
            //% "%1 set or disabled expiring messages timeout."
            : qsTrId("whisperfish-service-message-expiry-unknown-peer").arg(rcptName)
        }                        //: Group change: message expiry was changed
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

            return expiryMessage(_outgoing, peerName, secs)
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
            if (_data == null) {
                //: Service message
                //% "The group was updated."
                return qsTrId("whisperfish-service-message-changed-group")
            } else {
                switch (_data.change) {
                    case "add_banned_member":
                        //: Group change: add banned member
                        //% "%1 banned %2"
                    return qsTrId("whisperfish-service-message-group-change-add-banned-member").arg(peerName).arg(_data.value)
                    case "announcement_only":
                        return _data.value == "on" ?
                        //: Group change: only admins can send messages
                        //% "%1 restricted sending messages to administrators only"
                        qsTrId("whisperfish-service-message-group-change-announcement-only-on").arg(peerName).arg(_data.value) :
                        //: Group change: all members can send messages
                        //% "%1 allowed everyone send messages"
                        qsTrId("whisperfish-service-message-group-change-announcement-only-off").arg(peerName).arg(_data.value)
                    case "attribute_access":
                        // TODO: Better translations
                        //: Group change: permissions to change group properties
                        //% "%1 set group change permissions to '%2'"
                        return qsTrId("whisperfish-service-message-group-change-attribute-access").arg(peerName).arg(_data.value)
                    case "avatar":
                        //: Group change: title
                        //% "%1 changed the group avatar"
                        return qsTrId("whisperfish-service-message-group-change-avatar").arg(peerName)
                    case "delete_member":
                        //: Group change: delete member
                        //% "%1 removed %2 from the group"
                        return qsTrId("whisperfish-service-message-group-change-delete-member").arg(peerName).arg(_data.value)
                    case "description":
                        //: Group change: desctiption changed
                        //% "%1 changed the group description to '%2'"
                        return qsTrId("whisperfish-service-message-group-change-description").arg(peerName).arg(_data.value)
                    case "invite_link_access":
                        // TODO: Better translations
                        //: Group change: joining group via invite link setting
                        //% "%1 set invite link setting to '%2'"
                        return qsTrId("whisperfish-service-message-group-change-invite-link-access").arg(peerName).arg(_data.value)
                    case "invite_link_password":
                        //: Group change: set/change invite link password
                        //% "%1 changed the invite link password"
                        return qsTrId("whisperfish-service-message-group-change-invite-link-password").arg(peerName)
                    case "member_access":
                        //: Group change: change members joining setting
                        //% "%1 allowed '%2' add new members"
                        return qsTrId("whisperfish-service-message-group-change-member-access").arg(peerName).arg(_data.value)
                    case "modify_member_role":
                        //: Group change: change member "power level"
                        //% "%1 changed %2 to %3"
                        return qsTrId("whisperfish-service-message-group-change-modify-member-role").arg(peerName).arg(_data.aci).arg(_data.value)
                    case "new_member":
                        //: Group change: new member
                        //% "%1 added %2 to group"
                        return qsTrId("whisperfish-service-message-group-change-new-member").arg(peerName).arg(_data.aci)
                    case "new_pending_member":
                        //: Group change: new pending member
                        //% "%1 was invited to join the group"
                        return qsTrId("whisperfish-service-message-group-change-new-pending-member").arg(peerName).arg(_data.aci)
                    case "new_requesting_member":
                        //: Group change: new requesting member
                        //% "%1 would like to join the group"
                        return qsTrId("whisperfish-service-message-group-change-new-requesting-member").arg(peerName).arg(_data.aci)
                    case "promote_pending_member":
                        //: Group change: pending member was accepted
                        //% "%1 accepted %2 into the group"
                        return qsTrId("whisperfish-service-message-group-change-promote-pending-member").arg(peerName).arg(_data.aci)
                    case "promote_requesting_member":
                        //: Group change: requesting member was accepted
                        //% "%1 accepted %2 into the group"
                        return qsTrId("whisperfish-service-message-group-change-promote-requesting-member").arg(peerName).arg(_data.aci)
                    case "timer":
                        return expiryMessage(_outgoing, peerName, _data.value)
                    case "title":
                        //: Group change: title
                        //% "%1 changed the group title to '%2'"
                        return qsTrId("whisperfish-service-message-group-change-title").arg(peerName).arg(_data.value)
                }
            }
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
        case "group_call":
            return _outgoing
            //: Service message
            //% "You had a group call."
            ? qsTrId("whisperfish-service-message-call-group-self")
            //: Service message, %1 is the person initiating the call.
            //% "%1 had a group call with you."
            : qsTrId("whisperfish-service-message-call-group-peer").arg(peerName)
        case "missed_audio_call":
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
        case "outgoing_video_call":
            //: Service message, %1 is a name
            //% "You had a video call with %1."
            return qsTrId("whisperfish-service-message-call-video-self").arg(peerName)
        case "incoming_video_call":
            //: Service message, %1 is a name
            //% "%1 had a video call with you."
            return qsTrId("whisperfish-service-message-call-video-peer").arg(peerName)
        case "outgoing_audio_call":
            //: Service message, %1 is a name
            //% "You had a voice call with %1."
            return qsTrId("whisperfish-service-message-call-voice-self").arg(peerName)
        case "incoming_audio_call":
            //: Service message, %1 is a name
            //% "%1 had a voice call with you."
            return qsTrId("whisperfish-service-message-call-voice-peer").arg(peerName)
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
