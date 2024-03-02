// SPDX-FileCopyrightText: 2024 Matti Viljanen
// SPDX-License-Identifier: AGPL-3.0-or-later
import QtQuick 2.2
import Sailfish.Silica 1.0

Column {
    id: root
    property int duration: -1 // Expiring messages timeout, in seconds.
    property int newDuration

    function updateNewDuration() {
        switch (unitComboBox.unit) {
            case "s": newDuration = timeComboBox.amount;          break;
            case "m": newDuration = timeComboBox.amount * 60;     break;
            case "h": newDuration = timeComboBox.amount * 3600;   break;
            case "d": newDuration = timeComboBox.amount * 86400;  break;
            case "w": newDuration = timeComboBox.amount * 604800; break;
        }
    }

    function getModelCount(unit) {
        switch (unit) {
            case "s": return 59;
            case "m": return 59;
            case "h": return 23;
            case "d": return  6;
            case "w": return  4;
            default:  return 59;
        }
    }

    function getExpiryIndex(seconds) {
        switch (seconds) {
            case -1:      return 0; // Off
            case 30:      return 1; // 30s
            case 300:     return 2; // 5min
            case 3600:    return 3; // 1h
            case 28800:   return 4; // 8h
            case 86400:   return 5; // 1d
            case 604800:  return 6; // 1w
            case 2419200: return 7; // 4w
            default:      return 8; // custom
        }
    }

    function setTimeIndex() {
        timeComboBox.model = getModelCount(unitComboBox.unit)
        switch (unitComboBox.unit) {
            case "w": timeComboBox.currentIndex = Math.min( 4, timeComboBox.amount) - 1; newDuration = timeComboBox.amount * 604800; break;
            case "d": timeComboBox.currentIndex = Math.min( 6, timeComboBox.amount) - 1; newDuration = timeComboBox.amount * 86400;  break;
            case "h": timeComboBox.currentIndex = Math.min(23, timeComboBox.amount) - 1; newDuration = timeComboBox.amount * 3600;   break;
            case "m": timeComboBox.currentIndex = Math.min(59, timeComboBox.amount) - 1; newDuration = timeComboBox.amount * 60;     break;
            default:  timeComboBox.currentIndex = Math.min(59, timeComboBox.amount) - 1; newDuration = timeComboBox.amount;          break;
        }
    }

    ComboBox {
        id: expiryComboBox
        //: Group/conversation info page, expiring messages setting
        //% "Disappearing messages"
        label: qsTrId("whisperfish-disappearing-messages-setting")
        //: Group/conversation info page, expiring messages description
        //% "Set or disable message destruction after a certain time after reading. Only affects messages sent after changing this option."
        description: qsTrId("whisperfish-disappearing-messages-description")
        Component.onCompleted: {
            expiryComboBox.currentIndex = getExpiryIndex(duration)
            if (currentIndex == 8 && duration > 0) {
                newDuration = duration
            }
            handleClick()
        }
        property int previousIndex: 0
        property var durations: [-1, 30, 300, 3600, 28800, 86400, 604800, 2419200, 0]
        property var texts: [
            //: Disappearing messages: off
            //% "Off"
            qsTrId("whisperfish-disappearing-messages-off"),

            //: Disappearing messages duration in seconds
            //% "%n second(s)"
            qsTrId("whisperfish-disappearing-messages-seconds", 30),

            //: Disappearing messages duration in minutes
            //% "%n minute(s)"
            qsTrId("whisperfish-disappearing-messages-minutes", 5),

            //: Disappearing messages duration in hours
            //% "%n hour(s)"
            qsTrId("whisperfish-disappearing-messages-hours", 1),

            // See above
            qsTrId("whisperfish-disappearing-messages-hours", 8),

            //: Disappearing messages duration in days
            //% "%n day(s)"
            qsTrId("whisperfish-disappearing-messages-days", 1),

            //: Disappearing messages duration in weeks
            //% "%n week(s)"
            qsTrId("whisperfish-disappearing-messages-weeks", 1),

            // See above
            qsTrId("whisperfish-disappearing-messages-weeks", 4),

            //: Disappearing messages: custom duration
            //% "Other"
            qsTrId("whisperfish-disappearing-messages-custom")
        ]
        menu: ContextMenu {
            Repeater {
                model: 9
                MenuItem {
                    text: expiryComboBox.texts[index]
                    onClicked: {
                        expiryComboBox.previousIndex = expiryComboBox.currentIndex
                        expiryComboBox.currentIndex = index
                        expiryComboBox.handleClick()
                    }
                }
            }
        }

        function handleClick() {
            if (expiryComboBox.currentIndex == 0) {
                // Off = -1
                root.newDuration = -1
            } else if (expiryComboBox.currentIndex > 0 && expiryComboBox.currentIndex < 8) {
                // Handle the possible -1 value
                root.newDuration = Math.max(1, expiryComboBox.durations[expiryComboBox.currentIndex])
            } else if (expiryComboBox.currentIndex === 8 && expiryComboBox.previousIndex < 8) {
                switch (expiryComboBox.previousIndex) {
                    case 7: unitComboBox.currentIndex = 4; unitComboBox.unit = "w"; timeComboBox.model = getModelCount(unitComboBox.unit); timeComboBox.currentIndex =  4 - 1; break;
                    case 6: unitComboBox.currentIndex = 4; unitComboBox.unit = "w"; timeComboBox.model = getModelCount(unitComboBox.unit); timeComboBox.currentIndex =  1 - 1; break;
                    case 5: unitComboBox.currentIndex = 3; unitComboBox.unit = "d"; timeComboBox.model = getModelCount(unitComboBox.unit); timeComboBox.currentIndex =  1 - 1; break;
                    case 4: unitComboBox.currentIndex = 2; unitComboBox.unit = "h"; timeComboBox.model = getModelCount(unitComboBox.unit); timeComboBox.currentIndex =  8 - 1; break;
                    case 3: unitComboBox.currentIndex = 2; unitComboBox.unit = "h"; timeComboBox.model = getModelCount(unitComboBox.unit); timeComboBox.currentIndex =  1 - 1; break;
                    case 2: unitComboBox.currentIndex = 1; unitComboBox.unit = "m"; timeComboBox.model = getModelCount(unitComboBox.unit); timeComboBox.currentIndex =  5 - 1; break;
                    case 1: unitComboBox.currentIndex = 0; unitComboBox.unit = "s"; timeComboBox.model = getModelCount(unitComboBox.unit); timeComboBox.currentIndex = 30 - 1; break;
                    case 0: unitComboBox.currentIndex = 0; unitComboBox.unit = "s"; timeComboBox.model = getModelCount(unitComboBox.unit); timeComboBox.currentIndex = 30 - 1; break;
                }
                updateNewDuration()
            }
        }
    }

    ComboBox {
        id: timeComboBox
        // No animation needed, because expiryComboBox opens a sub-page
        visible: expiryComboBox.currentIndex == 8
        //: Disappearing messages, custom "time amount" label
        //% "Amount"
        label: qsTrId("whisperfish-disappearing-messages-amount")
        property int amount: currentIndex + 1
        property var model: getModelCount(unitComboBox.unit)
        menu: ContextMenu {
            Repeater {
                model: timeComboBox.model
                MenuItem {
                    text: index + 1
                    onClicked: {
                        timeComboBox.currentIndex = index
                        updateNewDuration()
                    }
                }
            }
        }
    }

    ComboBox {
        id: unitComboBox
        // No animation needed, because expiryComboBox opens a sub-page
        visible: expiryComboBox.currentIndex == 8
        //: Disappearing messages, custom "time length" label
        //% "Time unit"
        label: qsTrId("whisperfish-disappearing-messages-time-units")
        property string unit
        property var unitTexts: [
            //: Time unit: seconds
            //% "seconds"
            qsTrId("whisperfish-units-seconds", timeComboBox.amount),
            //: Time unit: minutes
            //% "minutes"
            qsTrId("whisperfish-units-minutes", timeComboBox.amount),
            //: Time unit: hours
            //% "hours"
            qsTrId("whisperfish-units-hours", timeComboBox.amount),
            //: Time unit: days
            //% "days"
            qsTrId("whisperfish-units-days", timeComboBox.amount),
            //: Time unit: weeks
            //% "weeks"
            qsTrId("whisperfish-units-weeks", timeComboBox.amount)
        ]
        menu: ContextMenu {
            Repeater {
                model: ["s","m","h","d","w"]
                MenuItem {
                    text: unitComboBox.unitTexts[index]
                    onClicked: {
                        unitComboBox.currentIndex = index
                        unitComboBox.unit = modelData
                        setTimeIndex()
                        updateNewDuration()
                    }
                }
            }
        }
    }
}
